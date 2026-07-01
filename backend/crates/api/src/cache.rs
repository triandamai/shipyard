use redis::aio::ConnectionManager;

const PREFIX: &str = "shipyard:cache:";

pub async fn get(redis: &Option<ConnectionManager>, key: &str) -> Option<String> {
    let mut conn = redis.as_ref()?.clone();
    let full_key = format!("{PREFIX}{key}");
    redis::cmd("GET")
        .arg(&full_key)
        .query_async::<Option<String>>(&mut conn)
        .await
        .ok()
        .flatten()
}

pub async fn set(redis: &Option<ConnectionManager>, key: &str, value: &str, ttl_secs: u64) {
    let Some(mut conn) = redis.as_ref().map(|r| r.clone()) else { return };
    let full_key = format!("{PREFIX}{key}");
    let _ = redis::cmd("SET")
        .arg(&full_key)
        .arg(value)
        .arg("EX")
        .arg(ttl_secs)
        .query_async::<()>(&mut conn)
        .await;
}

pub async fn del(redis: &Option<ConnectionManager>, key: &str) {
    let Some(mut conn) = redis.as_ref().map(|r| r.clone()) else { return };
    let full_key = format!("{PREFIX}{key}");
    let _ = redis::cmd("DEL")
        .arg(&full_key)
        .query_async::<()>(&mut conn)
        .await;
}
