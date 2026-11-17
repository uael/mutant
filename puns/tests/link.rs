//! Runs `examples/link.rs`.

#[path = "../examples/link.rs"]
mod link;

#[test]
#[cfg_attr(
  miri,
  ignore = "walks over hundreds of bytes, slow under Miri: the same paths (walks, regions, checksums) run under Miri in api, ethernet, ip_services and the other examples"
)]
fn link() {
  link::main();
}
