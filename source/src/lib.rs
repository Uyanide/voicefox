pub mod apple;
pub mod bili;
pub mod js;
pub mod local;
pub mod manager;
pub mod session;
pub mod sync;

pub mod fivesing;
pub mod jamendo;
pub mod joox;
pub mod kg;
pub mod kw;
pub mod mg;
pub mod qianqian;
pub mod soda;
pub mod tx;
pub mod wy;

mod crypto;
mod filter;
mod http;

pub fn configure_network(proxy_url: &str, timeout_secs: u64) {
    http::configure(proxy_url, timeout_secs);
}
