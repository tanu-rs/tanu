//! # GraphQL Client Module
//!
//! Tanu's GraphQL support provides an ergonomic builder for sending GraphQL
//! queries and mutations, with optional type-safe codegen via `graphql_client`.
//!
//! ## Usage (Mode 1: Runtime String Queries)
//!
//! ```rust,ignore
//! use tanu::http::Client;
//! use tanu::graphql;
//!
//! let client = Client::new();
//! let res = client
//!     .graphql("https://api.example.com/graphql")
//!     .query("{ users { id name } }")
//!     .variables(serde_json::json!({"limit": 10}))
//!     .send()
//!     .await?;
//!
//! let data: graphql::Response<serde_json::Value> = res.json().await?;
//! ```
//!
//! ## Usage (Mode 2: Type-Safe Codegen)
//!
//! ```rust,ignore
//! use graphql_client::GraphQLQuery;
//! use tanu::http::Client;
//! use tanu::graphql;
//!
//! #[derive(GraphQLQuery)]
//! #[graphql(
//!     schema_path = "src/graphql/schema.graphql",
//!     query_path = "src/graphql/get_users.graphql",
//! )]
//! struct GetUsers;
//!
//! let client = Client::new();
//! let res = client
//!     .graphql("https://api.example.com/graphql")
//!     .typed_query::<GetUsers>(get_users::Variables { limit: 10 })
//!     .send()
//!     .await?;
//!
//! let data: graphql::Response<get_users::ResponseData> = res.json().await?;
//! ```

// Re-export core graphql_client types for response deserialization and codegen.
pub use graphql_client::{GraphQLQuery, Response};

/// A GraphQL error returned by the server.
pub use graphql_client::Error as GraphqlError;

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
struct GraphqlRequest {
    query: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    variables: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "operationName")]
    operation_name: Option<String>,
}

/// Builder for GraphQL requests.
///
/// Created by [`Client::graphql`](crate::http::Client::graphql). Use
/// [`query`](Self::query) for runtime string queries or
/// [`typed_query`](Self::typed_query) for type-safe codegen queries.
pub struct GraphqlRequestBuilder {
    inner: crate::http::RequestBuilder,
    query: String,
    variables: Option<serde_json::Value>,
    operation_name: Option<String>,
}

impl GraphqlRequestBuilder {
    pub(crate) fn new(inner: crate::http::RequestBuilder) -> Self {
        Self {
            inner,
            query: String::new(),
            variables: None,
            operation_name: None,
        }
    }

    /// Set the GraphQL query or mutation string.
    pub fn query(mut self, query: impl Into<String>) -> Self {
        self.query = query.into();
        self
    }

    /// Set the GraphQL variables.
    pub fn variables(mut self, variables: serde_json::Value) -> Self {
        self.variables = Some(variables);
        self
    }

    /// Set the GraphQL operation name.
    pub fn operation_name(mut self, name: impl Into<String>) -> Self {
        self.operation_name = Some(name.into());
        self
    }

    /// Set a type-safe query body from a `#[derive(GraphQLQuery)]` type.
    ///
    /// This extracts the query string, operation name, and serializes the
    /// variables from the generated `Q::build_query(variables)` output.
    pub fn typed_query<Q: GraphQLQuery>(mut self, variables: Q::Variables) -> Self
    where
        Q::Variables: Serialize,
    {
        let body = Q::build_query(variables);
        self.query = body.query.to_string();
        self.operation_name = Some(body.operation_name.to_string());
        self.variables = serde_json::to_value(&body.variables).ok();
        self
    }

    /// Add a single request header.
    pub fn header<K, V>(mut self, key: K, value: V) -> Self
    where
        http::header::HeaderName: TryFrom<K>,
        <http::header::HeaderName as TryFrom<K>>::Error: Into<http::Error>,
        http::header::HeaderValue: TryFrom<V>,
        <http::header::HeaderValue as TryFrom<V>>::Error: Into<http::Error>,
    {
        self.inner = self.inner.header(key, value);
        self
    }

    /// Set Bearer token authentication.
    pub fn bearer_auth<T: std::fmt::Display>(mut self, token: T) -> Self {
        self.inner = self.inner.bearer_auth(token);
        self
    }

    /// Set Basic authentication.
    pub fn basic_auth<U: std::fmt::Display, P: std::fmt::Display>(
        mut self,
        username: U,
        password: Option<P>,
    ) -> Self {
        self.inner = self.inner.basic_auth(username, password);
        self
    }

    /// Set the request timeout.
    pub fn timeout(mut self, timeout: std::time::Duration) -> Self {
        self.inner = self.inner.timeout(timeout);
        self
    }

    /// Send the GraphQL request.
    pub async fn send(self) -> Result<crate::http::Response, crate::http::Error> {
        let gql_req = GraphqlRequest {
            query: self.query,
            variables: self.variables,
            operation_name: self.operation_name,
        };
        self.inner.json(&gql_req).send().await
    }
}
