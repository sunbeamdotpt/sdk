//! Test fixtures for Sunbeam services.

use std::collections::HashMap;

/// Fixture for creating test data.
pub trait Fixture: Send + Sync + 'static {
    /// The type this fixture creates.
    type Output;

    /// Generate a new instance.
    fn generate(&self) -> Self::Output;

    /// Generate multiple instances.
    fn generate_many(&self, count: usize) -> Vec<Self::Output> {
        (0..count).map(|_| self.generate()).collect()
    }
}

/// String fixture.
#[derive(Debug)]
pub struct StringFixture {
    /// Prefix for generated strings.
    prefix: String,
    /// Counter for unique strings.
    counter: std::sync::atomic::AtomicUsize,
}

impl StringFixture {
    /// Create a new string fixture.
    pub fn new(prefix: impl Into<String>) -> Self {
        Self {
            prefix: prefix.into(),
            counter: std::sync::atomic::AtomicUsize::new(0),
        }
    }
}

impl Default for StringFixture {
    fn default() -> Self {
        Self::new("test")
    }
}

impl Fixture for StringFixture {
    type Output = String;

    fn generate(&self) -> Self::Output {
        let count = self
            .counter
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        format!("{}_{}", self.prefix, count)
    }
}

/// UUID fixture.
#[derive(Debug, Clone)]
pub struct UuidFixture;

impl Fixture for UuidFixture {
    type Output = String;

    fn generate(&self) -> Self::Output {
        uuid::Uuid::new_v4().to_string()
    }
}

/// Integer fixture.
#[allow(dead_code)]
#[derive(Debug)]
pub struct IntFixture {
    min: i64,
    max: i64,
    counter: std::sync::atomic::AtomicI64,
}

impl IntFixture {
    /// Create a new integer fixture.
    pub fn new(min: i64, max: i64) -> Self {
        Self {
            min,
            max,
            counter: std::sync::atomic::AtomicI64::new(min),
        }
    }

    /// Create a sequential integer fixture.
    pub fn sequential(start: i64) -> Self {
        Self {
            min: start,
            max: i64::MAX,
            counter: std::sync::atomic::AtomicI64::new(start),
        }
    }
}

impl Default for IntFixture {
    fn default() -> Self {
        Self::new(0, 1000)
    }
}

impl Fixture for IntFixture {
    type Output = i64;

    fn generate(&self) -> Self::Output {
        self.counter
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
    }
}

/// Fixture for generating IDs.
#[derive(Debug)]
pub struct IdFixture {
    /// Prefix for IDs.
    prefix: String,
    /// Counter for unique IDs.
    counter: std::sync::atomic::AtomicUsize,
}

impl IdFixture {
    /// Create a new ID fixture.
    pub fn new(prefix: impl Into<String>) -> Self {
        Self {
            prefix: prefix.into(),
            counter: std::sync::atomic::AtomicUsize::new(1),
        }
    }
}

impl Default for IdFixture {
    fn default() -> Self {
        Self::new("id")
    }
}

impl Fixture for IdFixture {
    type Output = String;

    fn generate(&self) -> Self::Output {
        let count = self
            .counter
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        format!("{}-{}", self.prefix, count)
    }
}

/// Fixture for generating timestamps.
#[derive(Debug, Clone)]
pub struct TimestampFixture;

impl Fixture for TimestampFixture {
    type Output = chrono::DateTime<chrono::Utc>;

    fn generate(&self) -> Self::Output {
        chrono::Utc::now()
    }
}

/// Fixture for generating HTTP headers.
#[derive(Debug, Clone)]
pub struct HeadersFixture {
    /// Default headers to include.
    default_headers: HashMap<String, String>,
}

impl HeadersFixture {
    /// Create a new headers fixture.
    pub fn new() -> Self {
        Self {
            default_headers: HashMap::new(),
        }
    }

    /// Add a default header.
    pub fn with_header(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.default_headers.insert(key.into(), value.into());
        self
    }
}

impl Default for HeadersFixture {
    fn default() -> Self {
        Self::new()
    }
}

impl Fixture for HeadersFixture {
    type Output = HashMap<String, String>;

    fn generate(&self) -> Self::Output {
        self.default_headers.clone()
    }
}

/// Fixture for generating test users.
#[derive(Debug)]
pub struct UserFixture {
    /// ID fixture.
    id_fixture: IdFixture,
    /// Name fixture.
    name_fixture: StringFixture,
    /// Email fixture.
    email_fixture: StringFixture,
}

impl UserFixture {
    /// Create a new user fixture.
    pub fn new() -> Self {
        Self {
            id_fixture: IdFixture::new("user"),
            name_fixture: StringFixture::new("User"),
            email_fixture: StringFixture::new("user"),
        }
    }
}

impl Default for UserFixture {
    fn default() -> Self {
        Self::new()
    }
}

/// Test user.
#[derive(Debug, Clone)]
pub struct TestUser {
    /// User ID.
    pub id: String,
    /// User name.
    pub name: String,
    /// User email.
    pub email: String,
}

impl Fixture for UserFixture {
    type Output = TestUser;

    fn generate(&self) -> Self::Output {
        TestUser {
            id: self.id_fixture.generate(),
            name: self.name_fixture.generate(),
            email: format!("{}@example.com", self.email_fixture.generate()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_string_fixture() {
        let fixture = StringFixture::new("test");
        assert_eq!(fixture.generate(), "test_0");
        assert_eq!(fixture.generate(), "test_1");
        assert_eq!(fixture.generate(), "test_2");
    }

    #[test]
    fn test_uuid_fixture() {
        let fixture = UuidFixture;
        let uuid1 = fixture.generate();
        let uuid2 = fixture.generate();
        assert_ne!(uuid1, uuid2);
        assert!(uuid::Uuid::parse_str(&uuid1).is_ok());
    }

    #[test]
    fn test_int_fixture_sequential() {
        let fixture = IntFixture::sequential(1);
        assert_eq!(fixture.generate(), 1);
        assert_eq!(fixture.generate(), 2);
        assert_eq!(fixture.generate(), 3);
    }

    #[test]
    fn test_id_fixture() {
        let fixture = IdFixture::new("user");
        assert_eq!(fixture.generate(), "user-1");
        assert_eq!(fixture.generate(), "user-2");
    }

    #[test]
    fn test_headers_fixture() {
        let fixture = HeadersFixture::new().with_header("Content-Type", "application/json");
        let headers = fixture.generate();
        assert_eq!(
            headers.get("Content-Type"),
            Some(&"application/json".to_string())
        );
    }

    #[test]
    fn test_user_fixture() {
        let fixture = UserFixture::new();
        let user = fixture.generate();
        assert!(user.id.starts_with("user-"));
        assert!(user.name.starts_with("User"));
        assert!(user.email.ends_with("@example.com"));
    }

    #[test]
    fn test_fixture_generate_many() {
        let fixture = IntFixture::sequential(1);
        let values: Vec<i64> = fixture.generate_many(5);
        assert_eq!(values, vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn test_fixture_defaults_and_ranges() {
        assert_eq!(StringFixture::default().generate(), "test_0");
        assert_eq!(IdFixture::default().generate(), "id-1");

        let ints = IntFixture::default();
        assert_eq!(ints.generate(), 0);
        let ranged = IntFixture::new(-2, 10);
        assert_eq!(ranged.generate(), -2);

        let timestamp = TimestampFixture.generate();
        assert!(timestamp <= chrono::Utc::now());

        let headers = HeadersFixture::default().generate();
        assert!(headers.is_empty());

        let user = UserFixture::default().generate();
        assert_eq!(user.id, "user-1");
        assert_eq!(user.name, "User_0");
        assert_eq!(user.email, "user_0@example.com");
    }
}
