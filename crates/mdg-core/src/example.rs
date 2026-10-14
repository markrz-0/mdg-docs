//! The reference "authentication" architecture (`examples/authentication.mdg`).
//!
//! ```text
//! Application (root)
//!   Frontend
//!   API Service
//!     Authentication
//!       Credential Validator
//!       Token Issuer
//!     User Routes
//!       Protected Endpoint
//!   JWT Validator
//! ```
//!
//! A token validation request originates inside Protected Endpoint, leaves
//! through the User Routes and API Service output boundaries, reaches JWT
//! Validator at root level, and the result returns through the matching
//! input boundaries. IDs are fixed so the generated file is reproducible.

use crate::model::{Document, Endpoint, PortDirection, Position};
use crate::ops::{
    add_connection, add_module, add_port, assert_valid, create_document, set_module_position, AddConnectionOptions,
    AddModuleOptions, AddPortOptions, CreateDocumentOptions,
};

struct Builder {
    doc: Document,
}

impl Builder {
    fn module(&mut self, id: &str, parent_id: &str, name: &str, description: &[&str], position: (f64, f64)) {
        self.doc = add_module(
            &self.doc,
            AddModuleOptions {
                id: Some(id.into()),
                parent_id: parent_id.into(),
                name: name.into(),
                description: Some(description.join("\n")),
                ..Default::default()
            },
        )
        .expect("example module")
        .document;
        self.doc = set_module_position(&self.doc, id, Position::new(position.0, position.1)).expect("example position");
    }

    fn port(&mut self, direction: PortDirection, module_id: &str, id: &str, name: &str, description: &str) {
        self.doc = add_port(
            &self.doc,
            module_id,
            direction,
            AddPortOptions {
                id: Some(id.into()),
                name: name.into(),
                description: Some(description.into()),
                ..Default::default()
            },
        )
        .expect("example port")
        .document;
    }

    fn input(&mut self, module_id: &str, id: &str, name: &str, description: &str) {
        self.port(PortDirection::Input, module_id, id, name, description);
    }

    fn output(&mut self, module_id: &str, id: &str, name: &str, description: &str) {
        self.port(PortDirection::Output, module_id, id, name, description);
    }

    fn connect(&mut self, id: &str, from: &str, to: &str) {
        let endpoint = |spec: &str| {
            let (m, p) = spec.split_once(':').expect("module:port");
            Endpoint::new(m, p)
        };
        self.doc = add_connection(
            &self.doc,
            AddConnectionOptions { id: Some(id.into()), from: endpoint(from), to: endpoint(to), ..Default::default() },
        )
        .expect("example connection")
        .document;
    }
}

pub fn create_authentication_example() -> Document {
    let doc = create_document(CreateDocumentOptions {
        title: Some("Authentication Example".into()),
        description: Some("Reference MDG architecture: a web application with token-based authentication.".into()),
        root_id: Some("application".into()),
        root_name: Some("Application".into()),
        root_description: Some(
            [
                "A small web application with a browser frontend, an HTTP API and a",
                "stateless JWT validator.",
                "",
                "Connections describe *possible* message flow only. Ordering, retries and",
                "error handling are described in each module.",
            ]
            .join("\n"),
        ),
        ..Default::default()
    })
    .expect("example document");
    let mut b = Builder { doc };

    // --- Root level -----------------------------------------------------------
    b.module(
        "frontend",
        "application",
        "Frontend",
        &[
            "Single-page browser application.",
            "",
            "- Renders the login form and the user profile page.",
            "- Stores the session token in memory only (never in `localStorage`).",
        ],
        (0.0, 80.0),
    );
    b.output(
        "frontend",
        "login-submitted",
        "Login Submitted",
        "Emits `POST /login` with a username and password typed by the user.",
    );
    b.output(
        "frontend",
        "profile-requested",
        "Profile Requested",
        "Emits `GET /profile` with the session token in the `Authorization` header.",
    );
    b.input(
        "frontend",
        "session-established",
        "Session Established",
        "Receives the login outcome: a signed JWT on success, or an error message on failure.",
    );
    b.input(
        "frontend",
        "profile-received",
        "Profile Received",
        "Receives the user profile, or `401 Unauthorized` when the token was rejected.",
    );

    b.module(
        "api-service",
        "application",
        "API Service",
        &[
            "HTTP API exposed to the frontend.",
            "",
            "Delegates authentication to its **Authentication** submodule and serves",
            "protected resources through **User Routes**. Token validation is",
            "delegated to the shared **JWT Validator** outside this service.",
        ],
        (380.0, 40.0),
    );
    b.input(
        "api-service",
        "login-request",
        "Login Request",
        "Receives `POST /login` requests containing user credentials.",
    );
    b.input(
        "api-service",
        "profile-request",
        "Profile Request",
        "Receives `GET /profile` requests carrying a bearer token.",
    );
    b.input(
        "api-service",
        "token-validation-result",
        "Token Validation Result",
        "Receives the verdict for a previously sent token validation request.",
    );
    b.output(
        "api-service",
        "login-response",
        "Login Response",
        "Emits the login outcome: a JWT on success or `401` with a reason.",
    );
    b.output(
        "api-service",
        "profile-response",
        "Profile Response",
        "Emits the profile document, or `401` when the token is invalid.",
    );
    b.output(
        "api-service",
        "token-validation-request",
        "Token Validation Request",
        "Emits a bearer token that must be verified before a protected resource is served.",
    );

    b.module(
        "jwt-validator",
        "application",
        "JWT Validator",
        &[
            "Stateless service that verifies JWT signatures and expiry.",
            "",
            "Shared by every service that serves protected resources, so it lives at",
            "the root level rather than inside the API Service.",
        ],
        (820.0, 120.0),
    );
    b.input("jwt-validator", "validate-token", "Validate Token", "Receives a raw bearer token to verify.");
    b.output(
        "jwt-validator",
        "validation-result",
        "Validation Result",
        "Emits `valid` with the token claims, or `invalid` with a reason (expired, bad signature, malformed).",
    );

    // --- API Service internals ------------------------------------------------
    b.module(
        "authentication",
        "api-service",
        "Authentication",
        &["Turns a set of credentials into a signed session token."],
        (0.0, 0.0),
    );
    b.input("authentication", "login-request", "Login Request", "Receives credentials from `POST /login`.");
    b.output("authentication", "token-issued", "Token Issued", "Emits a signed JWT.");
    b.output(
        "authentication",
        "login-failed",
        "Login Failed",
        "Emits a rejection when the credentials are wrong or the account is locked.",
    );

    b.module(
        "user-routes",
        "api-service",
        "User Routes",
        &["Routes for user-owned resources. Every route requires a valid token."],
        (0.0, 240.0),
    );
    b.input("user-routes", "profile-request", "Profile Request", "Receives `GET /profile` requests.");
    b.input(
        "user-routes",
        "token-validation-result",
        "Token Validation Result",
        "Receives the verdict for a token sent out for validation.",
    );
    b.output("user-routes", "profile-response", "Profile Response", "Emits the profile or a `401` response.");
    b.output(
        "user-routes",
        "token-validation-request",
        "Token Validation Request",
        "Emits a bearer token that needs verification.",
    );

    // --- Authentication internals ---------------------------------------------
    b.module(
        "credential-validator",
        "authentication",
        "Credential Validator",
        &[
            "Checks a username/password pair against the user store.",
            "",
            "Passwords are compared using a constant-time hash comparison.",
        ],
        (0.0, 40.0),
    );
    b.input("credential-validator", "check-credentials", "Check Credentials", "Receives a username and password.");
    b.output("credential-validator", "credentials-valid", "Credentials Valid", "Emits the authenticated user ID.");
    b.output("credential-validator", "credentials-invalid", "Credentials Invalid", "Emits a rejection reason.");

    b.module(
        "token-issuer",
        "authentication",
        "Token Issuer",
        &["Signs a JWT for an authenticated user. Tokens expire after 15 minutes."],
        (320.0, 0.0),
    );
    b.input("token-issuer", "issue-token", "Issue Token", "Receives an authenticated user ID.");
    b.output("token-issuer", "token-issued", "Token Issued", "Emits the signed JWT.");

    // --- User Routes internals ------------------------------------------------
    b.module(
        "protected-endpoint",
        "user-routes",
        "Protected Endpoint",
        &[
            "Serves `GET /profile`.",
            "",
            "1. Extracts the bearer token from the request.",
            "2. Sends it out for validation.",
            "3. When the validation result is `valid`, loads and returns the profile;",
            "   otherwise responds with `401 Unauthorized`.",
            "",
            "The numbered steps describe behaviour; MDG connections themselves do not",
            "imply any ordering.",
        ],
        (0.0, 40.0),
    );
    b.input("protected-endpoint", "http-request", "HTTP Request", "Receives `GET /profile` with a bearer token.");
    b.input(
        "protected-endpoint",
        "token-validation-result",
        "Token Validation Result",
        "Receives the validation verdict for the extracted token.",
    );
    b.output(
        "protected-endpoint",
        "token-validation-request",
        "Token Validation Request",
        "Emits the extracted bearer token for verification.",
    );
    b.output(
        "protected-endpoint",
        "http-response",
        "HTTP Response",
        "Emits the profile document or `401 Unauthorized`.",
    );

    // --- Connections ----------------------------------------------------------
    // Root
    b.connect("root-login", "frontend:login-submitted", "api-service:login-request");
    b.connect("root-login-response", "api-service:login-response", "frontend:session-established");
    b.connect("root-profile", "frontend:profile-requested", "api-service:profile-request");
    b.connect("root-profile-response", "api-service:profile-response", "frontend:profile-received");
    b.connect("root-validate-token", "api-service:token-validation-request", "jwt-validator:validate-token");
    b.connect("root-validation-result", "jwt-validator:validation-result", "api-service:token-validation-result");

    // API Service
    b.connect("api-login", "api-service:login-request", "authentication:login-request");
    b.connect("api-token-issued", "authentication:token-issued", "api-service:login-response");
    b.connect("api-login-failed", "authentication:login-failed", "api-service:login-response");
    b.connect("api-profile", "api-service:profile-request", "user-routes:profile-request");
    b.connect("api-profile-response", "user-routes:profile-response", "api-service:profile-response");
    b.connect("api-validate-token", "user-routes:token-validation-request", "api-service:token-validation-request");
    b.connect("api-validation-result", "api-service:token-validation-result", "user-routes:token-validation-result");

    // Authentication
    b.connect("auth-check", "authentication:login-request", "credential-validator:check-credentials");
    b.connect("auth-issue", "credential-validator:credentials-valid", "token-issuer:issue-token");
    b.connect("auth-token", "token-issuer:token-issued", "authentication:token-issued");
    b.connect("auth-failed", "credential-validator:credentials-invalid", "authentication:login-failed");

    // User Routes
    b.connect("routes-request", "user-routes:profile-request", "protected-endpoint:http-request");
    b.connect("routes-response", "protected-endpoint:http-response", "user-routes:profile-response");
    b.connect(
        "routes-validate-token",
        "protected-endpoint:token-validation-request",
        "user-routes:token-validation-request",
    );
    b.connect(
        "routes-validation-result",
        "user-routes:token-validation-result",
        "protected-endpoint:token-validation-result",
    );

    assert_valid(&b.doc).expect("the example is valid");
    b.doc
}
