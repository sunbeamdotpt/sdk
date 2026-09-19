import { describe, it, beforeEach } from "@std/testing/bdd";
import { expect } from "@std/expect";
import { authActions } from "../state/auth.ts";
import { withAuthGuard, withRoleGuard } from "./guards.ts";

describe("withAuthGuard", () => {
  beforeEach(() => {
    authActions.logout();
  });

  it("throws redirect when anonymous", () => {
    const guard = withAuthGuard({ redirectTo: "/login" });
    let threw = false;
    try {
      guard();
    } catch {
      threw = true;
    }
    expect(threw).toBe(true);
  });

  it("does not throw when authenticated", () => {
    authActions.loginSuccess({
      accessToken: "token",
      claims: { sub: "user-1" },
    });
    const guard = withAuthGuard({ redirectTo: "/login" });
    expect(() => guard()).not.toThrow();
  });
});

describe("withRoleGuard", () => {
  beforeEach(() => {
    authActions.logout();
  });

  it("throws redirect when anonymous", () => {
    const guard = withRoleGuard(["admin"]);
    let threw = false;
    try {
      guard();
    } catch {
      threw = true;
    }
    expect(threw).toBe(true);
  });

  it("throws redirect when missing role", () => {
    authActions.loginSuccess({
      accessToken: "token",
      claims: { sub: "user-1", roles: ["user"] },
    });
    const guard = withRoleGuard(["admin"]);
    let threw = false;
    try {
      guard();
    } catch {
      threw = true;
    }
    expect(threw).toBe(true);
  });

  it("does not throw when role matches", () => {
    authActions.loginSuccess({
      accessToken: "token",
      claims: { sub: "user-1", roles: ["admin"] },
    });
    const guard = withRoleGuard(["admin"]);
    expect(() => guard()).not.toThrow();
  });
});
