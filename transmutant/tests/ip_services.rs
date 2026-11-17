//! Runs `examples/ip_services.rs`.

#[path = "../examples/ip_services.rs"]
mod ip_services;

#[test]
fn ip_services() {
  ip_services::main();
}
