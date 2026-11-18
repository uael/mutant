//! Where the analysis gives up on proving (the source is text, whose bytes go on to its end),
//! everything of the target is checked at run time: which a pointer cannot be.
#[path = "../support/relative.rs"]
#[allow(dead_code)]
mod relative;
use relative::Opt;

fn main() {
  let _ = puns::try_view_ref::<str, Opt<u32>>("text");
}
