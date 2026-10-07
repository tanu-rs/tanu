use graphql_client::GraphQLQuery;
use tanu::{check, check_eq, eyre, graphql, http::Client};

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "src/graphql/schema.graphql",
    query_path = "src/graphql/get_user.graphql",
    response_derives = "Debug"
)]
struct GetUser;

/// A GraphQL query is posted as the `query` field of a JSON body.
///
/// httpbin /post echoes back: { "json": <posted body>, "headers": {...}, ... }
#[tanu::test]
async fn graphql_basic_query() -> eyre::Result<()> {
    let http = Client::new();
    let base_url = crate::get_base_url().await?;

    let res = http
        .graphql(format!("{base_url}/post"))
        .query("{ users { id name } }")
        .send()
        .await?;

    check_eq!(200, res.status().as_u16());
    let body: serde_json::Value = res.json().await?;
    check_eq!(body["json"]["query"], "{ users { id name } }");
    Ok(())
}

/// The query, variables and operation name are posted as the GraphQL JSON body.
#[tanu::test]
async fn graphql_query_with_variables() -> eyre::Result<()> {
    let http = Client::new();
    let base_url = crate::get_base_url().await?;

    let res = http
        .graphql(format!("{base_url}/post"))
        .query("query GetUser($id: ID!) { user(id: $id) { name } }")
        .variables(serde_json::json!({"id": "42"}))
        .operation_name("GetUser")
        .send()
        .await?;

    check_eq!(200, res.status().as_u16());
    let body: serde_json::Value = res.json().await?;
    check_eq!(
        body["json"]["query"],
        "query GetUser($id: ID!) { user(id: $id) { name } }"
    );
    check_eq!(body["json"]["variables"]["id"], "42");
    check_eq!(body["json"]["operationName"], "GetUser");
    Ok(())
}

/// A bearer token on a GraphQL request is sent as the `Authorization` header.
#[tanu::test]
async fn graphql_with_bearer_auth() -> eyre::Result<()> {
    let http = Client::new();
    let base_url = crate::get_base_url().await?;

    let res = http
        .graphql(format!("{base_url}/post"))
        .query("{ me { id } }")
        .bearer_auth("my-secret-token")
        .send()
        .await?;

    check_eq!(200, res.status().as_u16());
    let body: serde_json::Value = res.json().await?;
    check!(
        body["headers"]["Authorization"]
            .as_str()
            .unwrap_or("")
            .starts_with("Bearer "),
        "Authorization header should be Bearer"
    );
    Ok(())
}

/// `graphql::Response<T>` deserializes a body with no `data` or `errors`.
#[tanu::test]
async fn graphql_response_type() -> eyre::Result<()> {
    let http = Client::new();
    let base_url = crate::get_base_url().await?;

    // httpbin /post echoes the body; the response isn't a real GraphQL response,
    // but we verify that graphql::Response<T> deserializes correctly when
    // the body matches the {data, errors} shape.
    let res = http
        .graphql(format!("{base_url}/post"))
        .query("{ users { id } }")
        .send()
        .await?;

    check_eq!(200, res.status().as_u16());
    // graphql::Response<T> has optional data and errors fields.
    // httpbin returns a non-GraphQL response, so data will be None — that's fine.
    let gql_res: graphql::Response<serde_json::Value> = res.json().await?;
    // httpbin echoes back the request, not a GraphQL response, so errors is None.
    check!(gql_res.errors.is_none());
    Ok(())
}

/// A typed `graphql_client` query posts its operation name, variables and query text.
#[tanu::test]
async fn graphql_typed_query() -> eyre::Result<()> {
    let http = Client::new();
    let base_url = crate::get_base_url().await?;

    let res = http
        .graphql(format!("{base_url}/post"))
        .typed_query::<GetUser>(get_user::Variables {
            id: "42".to_string(),
        })
        .send()
        .await?;

    check_eq!(200, res.status().as_u16());
    let body: serde_json::Value = res.json().await?;
    // graphql_client generates the operation name from the struct name
    check_eq!(body["json"]["operationName"], "GetUser");
    check_eq!(body["json"]["variables"]["id"], "42");
    // The generated query string should contain the field selections
    check!(
        body["json"]["query"]
            .as_str()
            .unwrap_or("")
            .contains("GetUser"),
        "Query should contain the operation name"
    );
    Ok(())
}
