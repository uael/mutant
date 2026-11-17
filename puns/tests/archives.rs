//! Runs `examples/archives.rs`.

#[path = "../examples/archives.rs"]
mod archives;

#[test]
#[cfg_attr(
  miri,
  ignore = "walks over hundreds of bytes, slow under Miri: the same paths (walks, regions, checksums) run under Miri in api, ethernet, ip_services and the other examples"
)]
fn archives() {
  archives::main();
}
