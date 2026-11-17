//! Runs `examples/routing.rs`.

#[path = "../examples/routing.rs"]
mod routing;

#[test]
#[cfg_attr(
  miri,
  ignore = "walks over hundreds of bytes, slow under Miri: the same paths (walks, regions, checksums) run under Miri in api, ethernet, ip_services and the other examples"
)]
fn routing() {
  routing::main();
}
