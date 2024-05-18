use std::io::Write;

fn main() {
  let out_dir = std::env::var("OUT_DIR").unwrap();
  let dest_path = std::path::Path::new(&out_dir).join("impls.rs");
  let mut f = std::fs::File::create(&dest_path).unwrap();

  for i in 2..32 {
    let types = (1..i).map(|i| format!("T{}", i));

    let generics = types.clone().map(|ty| format!("{ty}: BitsPattern")).collect::<Vec<_>>().join(", ");
    let types = types.collect::<Vec<_>>().join(", ");
    let rhs = |op, types| match i {
      2 => "T1".to_string(),
      _ => format!("{op}<({types})>")
    };

    write!(
      &mut f,
      "impl<T0: BitsPattern, {generics}> BitsPattern for Any<(T0, {types})> {{ \
        const TAG: Tag = TAny; \
        type Lhs = T0; \
        type Rhs = {}; \
      }}\n\
      impl<T0: BitsPattern, {generics}> BitsPattern for All<(T0, {types})> {{ \
        const TAG: Tag = TAll; \
        type Lhs = T0; \
        type Rhs = {}; \
      }}\n\
      impl<T0: BitsPattern, {generics}> BitsPattern for Join<(T0, {types})> {{ \
        const TAG: Tag = TJoin; \
        type Lhs = T0; \
        type Rhs = {}; \
      }}\n",
      rhs("Any", &types),
      rhs("All", &types),
      rhs("Join", &types),
    ).unwrap();
  }
}