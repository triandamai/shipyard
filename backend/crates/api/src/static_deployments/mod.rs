pub mod api;
pub mod models;
pub mod nomi;
pub mod serving;
pub mod storage;
pub mod traefik;
pub mod upload;
pub mod verify;

pub use api::routes as api_routes;
pub use nomi::routes as nomi_routes;
pub use serving::routes as serving_routes;

#[cfg(test)]
mod tests;
