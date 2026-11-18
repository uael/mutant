//! Where the analysis gives up on proving (the source is text, whose bytes go on to its end),
//! everything of the target is checked at run time: which a pointer cannot be.
use nearest::Near;

fn main() {
  let _ = puns::try_view_ref::<str, Option<Near<u32>>>("text");
}
