use sqlx::PgPool;

use crate::tests::start_app;

#[sqlx::test]
async fn server_starts(pool: PgPool) {
    let (app, _) = start_app(pool).await;
    let client = &app.client;

    let response = client.get("healthz").send().await.expect("getting health");
    assert_eq!(response.status(), 200);

    drop(app.shutdown_tx);
    app.server_task
        .await
        .expect("server did not panic")
        .expect("server shutting down");
}

#[sqlx::test]
async fn route_not_found(pool: PgPool) {
    let (app, _) = start_app(pool).await;
    let client = &app.client;

    let response = client
        .get("this-route-is-nonexistent")
        .send()
        .await
        .expect("sending request");
    assert_eq!(response.status(), 404);
}
