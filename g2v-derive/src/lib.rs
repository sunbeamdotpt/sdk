// String-parsing helpers below predate let-chains; nesting is deliberate.
#![allow(clippy::collapsible_if)]

//! Proc-macro derive implementations for sunbeam-g2v framework.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use inflector::cases::snakecase::to_snake_case;
use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{DeriveInput, parse_macro_input};

/// Derive macro for `Instrumented` - generates a wrapper struct with metrics support
#[proc_macro_derive(Instrumented, attributes(instrument))]
pub fn derive_instrumented(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;
    let wrapper_name = format_ident!("Instrumented{}", name);

    quote! {
        #[derive(Debug, Clone)]
        pub struct #wrapper_name {
            inner: #name,
            metrics: Option<std::sync::Arc<::sdk::g2v::metrics::ServiceMetrics>>,
        }

        impl #wrapper_name {
            /// Create a new instrumented wrapper without metrics.
            pub fn new(inner: #name) -> Self {
                Self { inner, metrics: None }
            }

            /// Create a new instrumented wrapper with metrics.
            pub fn with_metrics(
                inner: #name,
                metrics: std::sync::Arc<::sdk::g2v::metrics::ServiceMetrics>,
            ) -> Self {
                Self { inner, metrics: Some(metrics) }
            }

            /// Consume the wrapper and return the inner value.
            pub fn into_inner(self) -> #name {
                self.inner
            }

            /// Get a reference to the inner value.
            pub fn inner(&self) -> &#name {
                &self.inner
            }

            /// Get a mutable reference to the inner value.
            pub fn inner_mut(&mut self) -> &mut #name {
                &mut self.inner
            }

            /// Get the metrics, if configured.
            pub fn metrics(&self) -> Option<&::sdk::g2v::metrics::ServiceMetrics> {
                self.metrics.as_deref()
            }
        }

        impl std::ops::Deref for #wrapper_name {
            type Target = #name;
            fn deref(&self) -> &Self::Target {
                &self.inner
            }
        }

        impl std::ops::DerefMut for #wrapper_name {
            fn deref_mut(&mut self) -> &mut Self::Target {
                &mut self.inner
            }
        }
    }
    .into()
}

/// Derive macro for generating Prometheus metrics from a struct
#[proc_macro_derive(Metrics, attributes(metrics, metric))]
pub fn derive_metrics(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;

    // Get struct-level attributes for prefix
    let prefix =
        extract_prefix_from_attrs(&input.attrs).unwrap_or_else(|| to_snake_case(&name.to_string()));

    // Process fields
    let fields = match &input.data {
        syn::Data::Struct(data) => match &data.fields {
            syn::Fields::Named(fields) => &fields.named,
            _ => panic!("Metrics derive only supports structs with named fields"),
        },
        _ => panic!("Metrics derive only supports structs"),
    };

    if fields.is_empty() {
        // No fields - just return a basic impl
        return quote! {
            impl #name {
                pub fn register_metrics(_registry: &prometheus::Registry) -> Result<Self, prometheus::Error> {
                    Ok(Self {})
                }
            }
        }.into();
    }

    // Generate metric definitions and getters
    let metric_defs: Vec<proc_macro2::TokenStream> = fields.iter().map(|field| {
        let Some(field_name) = field.ident.as_ref() else {
            return syn::Error::new_spanned(
                field,
                "#[derive(Metrics)] requires a struct with named fields",
            )
            .to_compile_error();
        };
        let field_name_str = field_name.to_string();

        // Get metric attributes from field
        let (metric_type, labels, buckets) = extract_field_metric_attrs(&field.attrs);

        let metric_name = format!("{}_{}", prefix, to_snake_case(&field_name_str));
        let metric_ident = format_ident!("{}", metric_name.to_uppercase().replace('-', "_"));

        // Generate label array expression
        let label_expr = if !labels.is_empty() {
            let label_lits: Vec<_> = labels.iter()
                .map(|l| {
                    let lit = syn::LitStr::new(l, proc_macro2::Span::call_site());
                    quote! { #lit }
                })
                .collect();
            quote! { &[#(#label_lits),*] }
        } else {
            quote! { &[] }
        };

        // Generate buckets expression
        let bucket_expr = if metric_type == Some(MetricType::Histogram) && !buckets.is_empty() {
            let bucket_lits: Vec<_> = buckets.iter()
                .map(|v| {
                    // Create a float literal expression using the value directly
                    quote! { #v }
                })
                .collect();
            quote! { &[#(#bucket_lits),*] }
        } else {
            quote! { prometheus::DEFAULT_BUCKETS }
        };

        // Generate metric definition
        let metric_def = match metric_type {
            Some(MetricType::Counter) => {
                if !labels.is_empty() {
                    quote! {
                        pub static #metric_ident: once_cell::sync::Lazy<prometheus::CounterVec> =
                            once_cell::sync::Lazy::new(|| {
                                prometheus::register_counter_vec!(
                                    #metric_name,
                                    concat!("Auto-generated counter for ", stringify!(#field_name)),
                                    #label_expr
                                ).expect("Failed to register counter vec")
                            });
                    }
                } else {
                    quote! {
                        pub static #metric_ident: once_cell::sync::Lazy<prometheus::Counter> =
                            once_cell::sync::Lazy::new(|| {
                                prometheus::register_counter!(
                                    #metric_name,
                                    concat!("Auto-generated counter for ", stringify!(#field_name))
                                ).expect("Failed to register counter")
                            });
                    }
                }
            }
            Some(MetricType::Gauge) => {
                if !labels.is_empty() {
                    quote! {
                        pub static #metric_ident: once_cell::sync::Lazy<prometheus::GaugeVec> =
                            once_cell::sync::Lazy::new(|| {
                                prometheus::register_gauge_vec!(
                                    #metric_name,
                                    concat!("Auto-generated gauge for ", stringify!(#field_name)),
                                    #label_expr
                                ).expect("Failed to register gauge vec")
                            });
                    }
                } else {
                    quote! {
                        pub static #metric_ident: once_cell::sync::Lazy<prometheus::Gauge> =
                            once_cell::sync::Lazy::new(|| {
                                prometheus::register_gauge!(
                                    #metric_name,
                                    concat!("Auto-generated gauge for ", stringify!(#field_name))
                                ).expect("Failed to register gauge")
                            });
                    }
                }
            }
            Some(MetricType::Histogram) => {
                if !labels.is_empty() {
                    quote! {
                        pub static #metric_ident: once_cell::sync::Lazy<prometheus::HistogramVec> =
                            once_cell::sync::Lazy::new(|| {
                                prometheus::register_histogram_vec!(
                                    #metric_name,
                                    concat!("Auto-generated histogram for ", stringify!(#field_name)),
                                    #label_expr
                                ).expect("Failed to register histogram vec")
                            });
                    }
                } else {
                    quote! {
                        pub static #metric_ident: once_cell::sync::Lazy<prometheus::Histogram> =
                            once_cell::sync::Lazy::new(|| {
                                prometheus::register_histogram!(
                                    #metric_name,
                                    concat!("Auto-generated histogram for ", stringify!(#field_name)),
                                    #bucket_expr
                                ).expect("Failed to register histogram")
                            });
                    }
                }
            }
            Some(MetricType::Summary) => {
                quote! {
                    pub static #metric_ident: once_cell::sync::Lazy<prometheus::Summary> =
                        once_cell::sync::Lazy::new(|| {
                            prometheus::register_summary!(
                                #metric_name,
                                concat!("Auto-generated summary for ", stringify!(#field_name))
                            ).expect("Failed to register summary")
                        });
                }
            }
            None => {
                // Default to Counter
                if !labels.is_empty() {
                    quote! {
                        pub static #metric_ident: once_cell::sync::Lazy<prometheus::CounterVec> =
                            once_cell::sync::Lazy::new(|| {
                                prometheus::register_counter_vec!(
                                    #metric_name,
                                    concat!("Auto-generated counter for ", stringify!(#field_name)),
                                    #label_expr
                                ).expect("Failed to register counter vec")
                            });
                    }
                } else {
                    quote! {
                        pub static #metric_ident: once_cell::sync::Lazy<prometheus::Counter> =
                            once_cell::sync::Lazy::new(|| {
                                prometheus::register_counter!(
                                    #metric_name,
                                    concat!("Auto-generated counter for ", stringify!(#field_name))
                                ).expect("Failed to register counter")
                            });
                    }
                }
            }
        };

        // Generate getter method
        let getter = match metric_type {
            Some(MetricType::Counter) => {
                if !labels.is_empty() {
                    quote! {
                        pub fn #field_name(&self) -> &'static prometheus::CounterVec {
                            &#metric_ident
                        }
                    }
                } else {
                    quote! {
                        pub fn #field_name(&self) -> &'static prometheus::Counter {
                            &#metric_ident
                        }
                    }
                }
            }
            Some(MetricType::Gauge) => {
                if !labels.is_empty() {
                    quote! {
                        pub fn #field_name(&self) -> &'static prometheus::GaugeVec {
                            &#metric_ident
                        }
                    }
                } else {
                    quote! {
                        pub fn #field_name(&self) -> &'static prometheus::Gauge {
                            &#metric_ident
                        }
                    }
                }
            }
            Some(MetricType::Histogram) => {
                if !labels.is_empty() {
                    quote! {
                        pub fn #field_name(&self) -> &'static prometheus::HistogramVec {
                            &#metric_ident
                        }
                    }
                } else {
                    quote! {
                        pub fn #field_name(&self) -> &'static prometheus::Histogram {
                            &#metric_ident
                        }
                    }
                }
            }
            Some(MetricType::Summary) => {
                quote! {
                    pub fn #field_name(&self) -> &'static prometheus::Summary {
                        &#metric_ident
                    }
                }
            }
            None => {
                if !labels.is_empty() {
                    quote! {
                        pub fn #field_name(&self) -> &'static prometheus::CounterVec {
                            &#metric_ident
                        }
                    }
                } else {
                    quote! {
                        pub fn #field_name(&self) -> &'static prometheus::Counter {
                            &#metric_ident
                        }
                    }
                }
            }
        };

        quote! {
            #metric_def
            #getter
        }
    }).collect();

    quote! {
        #(#metric_defs)*

        impl #name {
            /// Register all metrics with the given registry.
            pub fn register_metrics(_registry: &prometheus::Registry) -> Result<Self, prometheus::Error> {
                Ok(Self {})
            }
        }
    }
    .into()
}

// ============================================================================
// Helper types and functions
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MetricType {
    Counter,
    Gauge,
    Histogram,
    Summary,
}

/// Extract prefix from struct-level attributes
fn extract_prefix_from_attrs(attrs: &[syn::Attribute]) -> Option<String> {
    for attr in attrs {
        if !attr.path().is_ident("metrics") {
            continue;
        }
        // Use the meta field
        if let syn::Meta::List(meta_list) = &attr.meta {
            // meta_list.tokens is a TokenStream - convert to string
            let tokens_str = meta_list.tokens.to_string();
            if tokens_str.contains("prefix") && tokens_str.contains('"') {
                if let Some(start) = tokens_str.find('"') {
                    if let Some(end) = tokens_str[start + 1..].find('"') {
                        return Some(tokens_str[start + 1..start + 1 + end].to_string());
                    }
                }
            }
        }
    }
    None
}

/// Extract metric attributes from field
fn extract_field_metric_attrs(
    attrs: &[syn::Attribute],
) -> (Option<MetricType>, Vec<String>, Vec<f64>) {
    let mut metric_type = None;
    let mut labels = Vec::new();
    let mut buckets = Vec::new();

    for attr in attrs {
        if !attr.path().is_ident("metric") {
            continue;
        }

        let tokens_str = match &attr.meta {
            syn::Meta::List(meta_list) => meta_list.tokens.to_string(),
            syn::Meta::NameValue(nv) => format!("{:?}", nv),
            syn::Meta::Path(p) => format!("{:?}", p),
        };

        // Extract metric type
        if metric_type.is_none() {
            for keyword in ["counter", "gauge", "histogram", "summary"] {
                if tokens_str.to_lowercase().contains(keyword) {
                    metric_type = match keyword {
                        "counter" => Some(MetricType::Counter),
                        "gauge" => Some(MetricType::Gauge),
                        "histogram" => Some(MetricType::Histogram),
                        "summary" => Some(MetricType::Summary),
                        _ => None,
                    };
                    break;
                }
            }
            // Default to Counter
            if metric_type.is_none() {
                metric_type = Some(MetricType::Counter);
            }
        }

        // Extract labels
        if tokens_str.contains("labels") && tokens_str.contains('[') {
            if let Some(start) = tokens_str.find('[') {
                if let Some(end) = tokens_str[start..].find(']') {
                    let labels_str = &tokens_str[start + 1..start + end];
                    for label in labels_str.split(',') {
                        let trimmed = label.trim().trim_matches('"').trim_matches('\'');
                        if !trimmed.is_empty() {
                            labels.push(trimmed.to_string());
                        }
                    }
                }
            }
        }

        // Extract buckets
        if tokens_str.contains("buckets") && tokens_str.contains('[') {
            if let Some(start) = tokens_str.find("buckets") {
                let rest = &tokens_str[start..];
                if let Some(bracket_start) = rest.find('[') {
                    if let Some(bracket_end) = &rest[bracket_start..].find(']') {
                        let buckets_str = &rest[bracket_start + 1..bracket_start + *bracket_end];
                        for b in buckets_str.split(',') {
                            if let Ok(v) = b.trim().parse::<f64>() {
                                buckets.push(v);
                            }
                        }
                    }
                }
            }
        }
    }

    (metric_type, labels, buckets)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input() -> syn::DeriveInput {
        syn::parse_quote! {
            #[metrics(prefix = "custom")]
            struct Example {
                #[metric(counter, labels = ["service", "method"])]
                requests: u64,
                #[metric(gauge)]
                active: i64,
                #[metric(histogram, labels = ["route"], buckets = [0.1, 0.5, 1.0])]
                latency: f64,
                #[metric(summary)]
                payload: f64,
            }
        }
    }

    #[test]
    fn test_extract_prefix_from_attrs() {
        let parsed = input();
        assert_eq!(
            extract_prefix_from_attrs(&parsed.attrs).as_deref(),
            Some("custom")
        );
    }

    #[test]
    fn test_extract_prefix_ignores_unrelated_and_malformed_attrs() {
        let parsed: syn::DeriveInput = syn::parse_quote! {
            #[doc = "metrics(prefix = \"not-the-prefix\")"]
            #[metrics]
            struct Example;
        };
        assert_eq!(extract_prefix_from_attrs(&parsed.attrs), None);

        let no_attrs: syn::DeriveInput = syn::parse_quote!(
            struct Empty;
        );
        assert_eq!(extract_prefix_from_attrs(&no_attrs.attrs), None);
    }

    #[test]
    fn test_extract_field_metric_attrs_all_types() {
        let parsed = input();
        let syn::Data::Struct(data) = parsed.data else {
            panic!("expected struct");
        };
        let fields: Vec<_> = data.fields.iter().collect();

        assert_eq!(
            extract_field_metric_attrs(&fields[0].attrs),
            (
                Some(MetricType::Counter),
                vec!["service".to_string(), "method".to_string()],
                Vec::new()
            )
        );
        assert_eq!(
            extract_field_metric_attrs(&fields[1].attrs),
            (Some(MetricType::Gauge), Vec::new(), Vec::new())
        );
        assert_eq!(
            extract_field_metric_attrs(&fields[2].attrs),
            (
                Some(MetricType::Histogram),
                vec!["route".to_string()],
                vec![0.1, 0.5, 1.0]
            )
        );
        assert_eq!(
            extract_field_metric_attrs(&fields[3].attrs),
            (Some(MetricType::Summary), Vec::new(), Vec::new())
        );
    }

    #[test]
    fn test_extract_field_metric_attrs_defaults_and_filters() {
        let parsed: syn::DeriveInput = syn::parse_quote! {
            struct Example {
                plain: u64,
                #[metric]
                defaulted: u64,
                #[metric = "histogram"]
                name_value: f64,
                #[metric(counter, labels = ["", 's', "two"], buckets = ["bad", 2.5])]
                filtered: f64,
            }
        };
        let syn::Data::Struct(data) = parsed.data else {
            panic!("expected struct");
        };
        let fields: Vec<_> = data.fields.iter().collect();

        assert_eq!(
            extract_field_metric_attrs(&fields[0].attrs),
            (None, Vec::new(), Vec::new())
        );
        assert_eq!(
            extract_field_metric_attrs(&fields[1].attrs),
            (Some(MetricType::Counter), Vec::new(), Vec::new())
        );
        assert_eq!(
            extract_field_metric_attrs(&fields[2].attrs),
            (Some(MetricType::Histogram), Vec::new(), Vec::new())
        );
        assert_eq!(
            extract_field_metric_attrs(&fields[3].attrs),
            (
                Some(MetricType::Counter),
                vec!["s".to_string(), "two".to_string()],
                vec![2.5]
            )
        );
    }
}
