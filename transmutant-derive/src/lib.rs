//! `#[derive(Data)]`, see the documentation of the `transmutant` crate.

use proc_macro2::{Span, TokenStream};
use quote::{ToTokens, format_ident, quote};
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::spanned::Spanned;
use syn::{
  Attribute, BinOp, Data, DataEnum, DataStruct, DeriveInput, Error, Expr, ExprLit, ExprRange,
  Fields, Ident, Index, Lit, Member, Path, RangeLimits, Result, Token, Type, UnOp, Visibility,
  parse_macro_input, parse_quote,
};

/// Derive `Data`: tell the layout and the valid values of a type, so that it can be transmuted.
///
/// # Structs
///
/// Every field must be `Data`. Any `repr` will do, but a struct whose last field is unsized must
/// be `repr(C)` or `repr(transparent)`. The types of the fields must be at least as visible as
/// the struct (`pub` types for a `pub` struct, wherever they are declared).
///
/// Options of the struct:
/// - `#[data(size = EXPR)]`: the size in bytes of a value is `EXPR`;
/// - `#[data(assert(EXPR))]`: `EXPR` is true;
/// - `#[data(get(name: Type = EXPR, ..))]`: a method `name` that returns `EXPR`, for bit fields
///   and the like. Here `EXPR` is any Rust expression, in which the integer fields are `u128`;
/// - `#[data(tag(Key = Type, ..))]`: when the struct is carried by something that tells what it
///   carries with a `Key`, that `Key` is a `Type` (see the `Tag` trait);
/// - `#[data(crate = path)]`: path of the `transmutant` crate.
///
/// Options of a field:
/// - `#[data(valid(0x45..=0x4f, 0x60))]`: the field, an integer, only takes these values;
/// - `#[data(len = EXPR)]`, on the last field: its slice has `EXPR` elements;
/// - `#[data(regions(first = EXPR, second = EXPR, last))]`, on the last field: it is made of
///   consecutive regions of `EXPR` bytes, the last one may take what remains. A method that
///   returns the bytes of the region is generated for each name. With a type
///   (`first: Type = EXPR`, `last: Type`), the method returns the region as a `&Type`, if it is
///   one.
///
/// An `EXPR` is an expression over the integer fields of the struct (by name, or `self.0` for
/// tuple structs), `size_of_val(self)`, integer constants and the operators `+ - * / % & | ^ << >> == != < <= > >=
/// && || !`. It is compiled to a constant: it can be evaluated in `const fn`s, costs what the
/// arithmetic costs, and fails (as a rule that does not hold) on overflow.
///
/// It can name what it computes, and choose: `{ let name = EXPR; .. EXPR }` (at most 4 names at
/// once), `if EXPR { EXPR } else { EXPR }`.
///
/// It can also read the value beyond its fields, where `start`, `end` and `offset` are
/// expressions that count bytes from the start of the value:
/// - `le(offset, N)`, `be(offset, N)`: the integer of `N` bytes at `offset`;
/// - `crc32(start, end)`: the CRC-32 (of Ethernet) of the bytes from `start` to `end`;
/// - `sum16(start, end)`: the ones' complement sum of the 16 bits words from `start` to `end`
///   (`0xffff` over data that holds its own IP checksum);
/// - `walk(START, |at| COND, |at| NEXT)`, in an `assert`: what `START` becomes when it is
///   replaced by `NEXT` for as long as `COND` is true of it. This is how what is at an offset
///   that depends on what is before is reached: after options, at the end of a chain of headers
///   (`walk(0, |at| be(at, 1) != 0, |at| at + 2 + be(at + 1, 1))` is the offset of the first
///   "type, length, value" whose type is 0). A value is walked for at most as many steps as it
///   has bytes, and the rule fails beyond.
///
/// # Enums
///
/// Enums without fields, of any `repr`: their valid values are read from the compiler.
///
/// Enums with fields whose layout is specified: `repr(C)`, `repr(u8)` (or any other integer), or
/// both. Every field must be `Data`, and can have the `valid` option. What is known of such an
/// enum is its tag; making one out of anything but itself checks the tag, then what the fields
/// of the variant require.
///
/// Enums whose variants each hold a reference, such as
/// `enum Frame<'a> { Ipv4(&'a Ipv4), Arp(&'a Arp), Other(&'a [u8]) }`, are not data but *views* of
/// data that is one thing or another. For them, two functions are generated, which try the
/// variants in order: `unpack(bytes)` takes the first variant that `bytes` start with, and returns
/// it with what remains; `parse(bytes)` returns the first variant that `bytes` are, whole.
#[proc_macro_derive(Data, attributes(data))]
pub fn derive_data(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
  let input = parse_macro_input!(input as DeriveInput);
  let expanded = match &input.data {
    Data::Struct(data) => derive_struct(&input, data),
    Data::Enum(data) => derive_enum(&input, data),
    Data::Union(_) => Err(Error::new(Span::call_site(), "`Data` cannot be derived for unions")),
  };
  expanded.unwrap_or_else(Error::into_compile_error).into()
}

// -------------------------------------------------------------------------------------------------
// Attributes

#[derive(Default)]
struct Repr {
  c: bool,
  transparent: bool,
  /// `N` of `packed(N)`.
  packed: Option<TokenStream>,
  /// `u8`, `i32`...
  int: Option<Ident>,
}

fn parse_repr(attrs: &[Attribute]) -> Result<Repr> {
  let mut repr = Repr::default();
  for attr in attrs.iter().filter(|attr| attr.path().is_ident("repr")) {
    attr.parse_nested_meta(|meta| {
      if meta.path.is_ident("C") {
        repr.c = true;
      } else if meta.path.is_ident("transparent") {
        repr.transparent = true;
      } else if meta.path.is_ident("packed") {
        repr.packed = Some(if meta.input.peek(syn::token::Paren) {
          let content;
          syn::parenthesized!(content in meta.input);
          content.parse::<Lit>()?.into_token_stream()
        } else {
          quote!(1)
        });
      } else if meta.input.peek(syn::token::Paren) {
        // `align(N)`
        let content;
        syn::parenthesized!(content in meta.input);
        content.parse::<TokenStream>()?;
      } else if let Some(ident) = meta.path.get_ident() {
        const INTS: [&str; 12] =
          ["u8", "u16", "u32", "u64", "u128", "usize", "i8", "i16", "i32", "i64", "i128", "isize"];
        if INTS.iter().any(|int| ident == int) {
          repr.int = Some(ident.clone());
        }
      }
      Ok(())
    })?;
  }
  Ok(repr)
}

/// `name`, `name: Type`, `name = EXPR` or `name: Type = EXPR`
struct Region {
  name: Ident,
  ty: Option<Type>,
  size: Option<Expr>,
}

impl Parse for Region {
  fn parse(input: ParseStream) -> Result<Self> {
    let name = input.parse()?;
    let ty = if input.peek(Token![:]) {
      input.parse::<Token![:]>()?;
      Some(input.parse()?)
    } else {
      None
    };
    let size = if input.peek(Token![=]) {
      input.parse::<Token![=]>()?;
      Some(input.parse()?)
    } else {
      None
    };
    Ok(Self { name, ty, size })
  }
}

/// `Key = Type`
struct TagOption {
  key: Type,
  tag: Type,
}

impl Parse for TagOption {
  fn parse(input: ParseStream) -> Result<Self> {
    let key = input.parse()?;
    input.parse::<Token![=]>()?;
    Ok(Self { key, tag: input.parse()? })
  }
}

/// `name: Type = EXPR`
struct Getter {
  name: Ident,
  ty: Type,
  expr: Expr,
}

impl Parse for Getter {
  fn parse(input: ParseStream) -> Result<Self> {
    let name = input.parse()?;
    input.parse::<Token![:]>()?;
    let ty = input.parse()?;
    input.parse::<Token![=]>()?;
    Ok(Self { name, ty, expr: input.parse()? })
  }
}

#[derive(Default)]
struct Options {
  root: Option<TokenStream>,
  tags: Vec<TagOption>,
  getters: Vec<Getter>,
  size: Option<Expr>,
  asserts: Vec<Expr>,
  valid: Option<Punctuated<Expr, Token![,]>>,
  len: Option<Expr>,
  regions: Option<Punctuated<Region, Token![,]>>,
}

fn parse_options(attrs: &[Attribute]) -> Result<Options> {
  let mut options = Options::default();
  for attr in attrs.iter().filter(|attr| attr.path().is_ident("data")) {
    attr.parse_nested_meta(|meta| {
      if meta.path.is_ident("crate") {
        options.root = Some(if meta.input.peek(Token![=]) {
          meta.value()?.parse::<Path>()?.into_token_stream()
        } else {
          quote!(crate)
        });
      } else if meta.path.is_ident("size") {
        options.size = Some(meta.value()?.parse()?);
      } else if meta.path.is_ident("len") {
        options.len = Some(meta.value()?.parse()?);
      } else if meta.path.is_ident("assert") {
        let content;
        syn::parenthesized!(content in meta.input);
        options.asserts.push(content.parse()?);
      } else if meta.path.is_ident("tag") {
        let content;
        syn::parenthesized!(content in meta.input);
        options.tags.extend(Punctuated::<TagOption, Token![,]>::parse_terminated(&content)?);
      } else if meta.path.is_ident("get") {
        let content;
        syn::parenthesized!(content in meta.input);
        options.getters.extend(Punctuated::<Getter, Token![,]>::parse_terminated(&content)?);
      } else if meta.path.is_ident("valid") {
        let content;
        syn::parenthesized!(content in meta.input);
        options.valid = Some(Punctuated::parse_terminated(&content)?);
      } else if meta.path.is_ident("regions") {
        let content;
        syn::parenthesized!(content in meta.input);
        options.regions = Some(Punctuated::parse_terminated(&content)?);
      } else {
        return Err(meta.error("unknown `data` option"));
      }
      Ok(())
    })?;
  }
  Ok(options)
}

// -------------------------------------------------------------------------------------------------
// Expressions

struct FieldInfo<'a> {
  member: Member,
  ty: &'a Type,
  vis: &'a Visibility,
  options: Options,
}

/// Compile `expr` to the operations of a postfix expression.
/// The variables of an expression: its `let`s, and the parameters of the closures of a `walk`.
#[derive(Default)]
struct Scope {
  names: Vec<(Ident, u8)>,
  /// In a closure of a `walk`.
  walking: bool,
  /// For each `walk`: the number of operations of the longest of its two expressions.
  walks: Vec<usize>,
}

impl Scope {
  fn declare(&mut self, name: &Ident) -> Result<u8> {
    let index = self.names.len() as u8;
    if index >= 4 {
      return Err(Error::new(name.span(), "too many variables in an expression"));
    }
    self.names.push((name.clone(), index));
    Ok(index)
  }

  fn get(&self, name: &Ident) -> Option<u8> {
    self.names.iter().rev().find(|(declared, _)| declared == name).map(|(_, index)| *index)
  }
}

/// `{ let name = EXPR; .. EXPR }`
fn compile_block(
  block: &syn::Block, fields: &[FieldInfo], root: &TokenStream, ops: &mut Vec<TokenStream>,
  scope: &mut Scope,
) -> Result<()> {
  let (op, visible) = (quote!(#root::layout::Op), scope.names.len());
  let Some((last, statements)) = block.stmts.split_last() else {
    return Err(Error::new(block.span(), "a block has a value"));
  };
  for statement in statements {
    let syn::Stmt::Local(local) = statement else {
      return Err(Error::new(statement.span(), "expected `let name = EXPR;`"));
    };
    let name = match &local.pat {
      syn::Pat::Ident(pat) => &pat.ident,
      syn::Pat::Type(pat) if matches!(&*pat.pat, syn::Pat::Ident(_)) => match &*pat.pat {
        syn::Pat::Ident(pat) => &pat.ident,
        _ => unreachable!(),
      },
      pat => return Err(Error::new(pat.span(), "expected a name")),
    };
    let Some(syn::LocalInit { expr, diverge: None, .. }) = &local.init else {
      return Err(Error::new(local.span(), "expected `let name = EXPR;`"));
    };
    compile(expr, fields, root, ops, scope)?;
    let index = scope.declare(name)?;
    ops.push(quote!(#op::Set(#index)));
  }
  let syn::Stmt::Expr(value, None) = last else {
    return Err(Error::new(last.span(), "a block ends with its value"));
  };
  compile(value, fields, root, ops, scope)?;
  scope.names.truncate(visible);
  Ok(())
}

fn compile(
  expr: &Expr, fields: &[FieldInfo], root: &TokenStream, ops: &mut Vec<TokenStream>,
  scope: &mut Scope,
) -> Result<()> {
  let op = quote!(#root::layout::Op);
  let load = |member: &Member, ops: &mut Vec<TokenStream>| {
    let field = fields.iter().find(|field| &field.member == member)?;
    let ty = field.ty;
    ops.push(quote!(#op::load::<#ty>(::core::mem::offset_of!(Self, #member))));
    Some(())
  };
  match expr {
    Expr::Paren(expr) => compile(&expr.expr, fields, root, ops, scope)?,
    Expr::Group(expr) => compile(&expr.expr, fields, root, ops, scope)?,
    // Everything is an unsigned integer.
    Expr::Cast(expr) => compile(&expr.expr, fields, root, ops, scope)?,
    Expr::Lit(ExprLit { lit: Lit::Bool(lit), .. }) => {
      let value = lit.value as u128;
      ops.push(quote!(#op::Lit(#value)));
    }
    Expr::Block(block) => compile_block(&block.block, fields, root, ops, scope)?,
    // `if COND { EXPR } else { EXPR }`
    Expr::If(branch) => {
      let Some((_, otherwise)) = &branch.else_branch else {
        return Err(Error::new(branch.span(), "an `if` has an `else`"));
      };
      compile(&branch.cond, fields, root, ops, scope)?;
      compile_block(&branch.then_branch, fields, root, ops, scope)?;
      compile(otherwise, fields, root, ops, scope)?;
      ops.push(quote!(#op::Select));
    }
    // A variable.
    Expr::Path(path) if path.path.get_ident().is_some_and(|ident| scope.get(ident).is_some()) => {
      let index = path.path.get_ident().and_then(|ident| scope.get(ident));
      ops.push(quote!(#op::Get(#index)));
    }
    Expr::Path(path) if path.path.get_ident().is_some_and(|ident| {
      load(&Member::Named(ident.clone()), ops).is_some()
    }) => {}
    Expr::Field(field) if matches!(&*field.base, Expr::Path(path) if path.path.is_ident("self")) => {
      load(&field.member, ops)
        .ok_or_else(|| Error::new(field.member.span(), "no such field"))?;
    }
    Expr::Unary(unary) if matches!(unary.op, UnOp::Not(_)) => {
      compile(&unary.expr, fields, root, ops, scope)?;
      ops.push(quote!(#op::Not));
    }
    Expr::Binary(binary) => {
      let name = match binary.op {
        BinOp::Add(_) => "Add",
        BinOp::Sub(_) => "Sub",
        BinOp::Mul(_) => "Mul",
        BinOp::Div(_) => "Div",
        BinOp::Rem(_) => "Rem",
        BinOp::BitAnd(_) => "And",
        BinOp::BitOr(_) => "Or",
        BinOp::BitXor(_) => "Xor",
        BinOp::Shl(_) => "Shl",
        BinOp::Shr(_) => "Shr",
        BinOp::Eq(_) => "Eq",
        BinOp::Ne(_) => "Ne",
        BinOp::Lt(_) => "Lt",
        BinOp::Le(_) => "Le",
        BinOp::Gt(_) => "Gt",
        BinOp::Ge(_) => "Ge",
        BinOp::And(_) => "Both",
        BinOp::Or(_) => "Either",
        _ => return Err(Error::new(binary.op.span(), "unsupported operator")),
      };
      compile(&binary.left, fields, root, ops, scope)?;
      compile(&binary.right, fields, root, ops, scope)?;
      let name = Ident::new(name, binary.op.span());
      ops.push(quote!(#op::#name));
    }
    // `size_of_val(self)`
    Expr::Call(call)
      if call.args.len() == 1
        && matches!(&*call.func, Expr::Path(path)
          if path.path.segments.last().is_some_and(|segment| segment.ident == "size_of_val"))
        && matches!(&call.args[0], Expr::Path(path) if path.path.is_ident("self")) =>
    {
      ops.push(quote!(#op::Size));
    }
    // `crc32(start, end)`, `sum16(start, end)`: over the bytes of the value.
    Expr::Call(call) if call.args.len() == 2 && (is_call(call, "crc32") || is_call(call, "sum16")) => {
      compile(&call.args[0], fields, root, ops, scope)?;
      compile(&call.args[1], fields, root, ops, scope)?;
      ops.push(if is_call(call, "crc32") { quote!(#op::Crc32) } else { quote!(#op::Sum16) });
    }
    // `le(offset, size)`, `be(offset, size)`: an integer, anywhere in the value.
    Expr::Call(call) if call.args.len() == 2 && (is_call(call, "le") || is_call(call, "be")) => {
      compile(&call.args[0], fields, root, ops, scope)?;
      let (size, big) = (&call.args[1], is_call(call, "be"));
      ops.push(quote!(#op::At { size: #size, big: #big }));
    }
    // `walk(START, |at| COND, |at| NEXT)`: `START`, replaced by `NEXT` for as long as `COND`.
    Expr::Call(call) if call.args.len() == 3 && is_call(call, "walk") => {
      if scope.walking {
        return Err(Error::new(call.span(), "a `walk` cannot be in a `walk`"));
      }
      compile(&call.args[0], fields, root, ops, scope)?;
      let (mut var, mut programs) = (0, vec![]);
      for closure in [&call.args[1], &call.args[2]] {
        let parameter = match closure {
          Expr::Closure(closure) if closure.inputs.len() == 1 => match &closure.inputs[0] {
            syn::Pat::Ident(pat) => Some((&pat.ident, &closure.body)),
            _ => None,
          },
          _ => None,
        };
        let Some((name, body)) = parameter else {
          return Err(Error::new(closure.span(), "expected `|name| EXPR`"));
        };
        let (visible, mut program) = (scope.names.len(), vec![]);
        var = scope.declare(name)?;
        scope.walking = true;
        let compiled = compile(body, fields, root, &mut program, scope);
        scope.walking = false;
        scope.names.truncate(visible);
        compiled?;
        programs.push(program);
      }
      let (cond, next) = (&programs[0], &programs[1]);
      scope.walks.push(cond.len().max(next.len()));
      ops.push(quote!(#op::Walk {
        var: #var,
        cond: const { &[#(#cond),*] },
        next: const { &[#(#next),*] },
      }));
    }
    // Anything else must be a constant.
    expr => ops.push(quote!(#op::Lit((#expr) as u128))),
  }
  Ok(())
}

/// Whatever `call` is a call of the function `name`.
fn is_call(call: &syn::ExprCall, name: &str) -> bool {
  matches!(&*call.func, Expr::Path(path) if path.path.is_ident(name))
}

/// Turn the expression of a getter into Rust, where a field is its value as an `u128`.
fn rust(expr: &Expr, fields: &[FieldInfo], root: &TokenStream) -> TokenStream {
  let field = |member: &Member| {
    fields.iter().any(|field| &field.member == member).then(|| quote!(#root::int(&{ self.#member })))
  };
  match expr {
    Expr::Paren(expr) => {
      let inner = rust(&expr.expr, fields, root);
      quote!((#inner))
    }
    Expr::Group(expr) => rust(&expr.expr, fields, root),
    Expr::Cast(cast) => {
      let (inner, ty) = (rust(&cast.expr, fields, root), &cast.ty);
      quote!((#inner as #ty))
    }
    Expr::Path(path) => match path.path.get_ident().and_then(|ident| field(&Member::Named(ident.clone()))) {
      Some(field) => field,
      None => quote!(#path),
    },
    Expr::Field(access) if matches!(&*access.base, Expr::Path(path) if path.path.is_ident("self")) => {
      match field(&access.member) {
        Some(field) => field,
        None => quote!(#access),
      }
    }
    Expr::Unary(unary) => {
      let (op, inner) = (&unary.op, rust(&unary.expr, fields, root));
      quote!((#op #inner))
    }
    Expr::Binary(binary) => {
      let (left, right) = (rust(&binary.left, fields, root), rust(&binary.right, fields, root));
      let op = &binary.op;
      quote!((#left #op #right))
    }
    Expr::Call(call)
      if call.args.len() == 1
        && matches!(&*call.func, Expr::Path(path)
          if path.path.segments.last().is_some_and(|segment| segment.ident == "size_of_val"))
        && matches!(&call.args[0], Expr::Path(path) if path.path.is_ident("self")) =>
    {
      quote!((::core::mem::size_of_val(self) as u128))
    }
    expr => quote!(#expr),
  }
}

/// `&[Op, ...]`
/// An expression, as operations.
struct Expression {
  ops: TokenStream,
  /// Number of operations.
  len: usize,
  /// The number of operations of the longest of the two expressions of its `walk`, if it has
  /// one.
  walk: Option<usize>,
}

fn expression(expr: &Expr, fields: &[FieldInfo], root: &TokenStream) -> Result<Expression> {
  let (mut ops, mut scope) = (vec![], Scope::default());
  compile(expr, fields, root, &mut ops, &mut scope)?;
  if scope.walks.len() > 1 {
    return Err(Error::new(expr.span(), "an expression can only have one `walk`"));
  }
  Ok(Expression { len: ops.len(), ops: quote!(&[#(#ops),*]), walk: scope.walks.pop() })
}

/// An expression that is not the one of an `assert`: it cannot `walk`.
fn plain(expr: &Expr, fields: &[FieldInfo], root: &TokenStream) -> Result<TokenStream> {
  let expression = expression(expr, fields, root)?;
  match expression.walk {
    Some(_) => Err(Error::new(expr.span(), "`walk` is for `assert`")),
    None => Ok(expression.ops),
  }
}

/// `count`, as a type.
fn count_type(count: usize, root: &TokenStream) -> TokenStream {
  (0..count).fold(quote!(#root::Zero), |less, _| quote!(#root::More<#less>))
}

/// The ranges of `#[data(valid(..))]`, as `&'static [(u128, u128)]`.
fn valid_ranges(
  ranges: &Punctuated<Expr, Token![,]>, ty: &Type, root: &TokenStream,
) -> TokenStream {
  let option = |expr: &Option<Box<Expr>>| match expr {
    Some(expr) => quote!(::core::option::Option::Some((#expr) as u128)),
    None => quote!(::core::option::Option::None),
  };
  let count = ranges.len();
  let ranges = ranges.iter().map(|range| match range {
    Expr::Range(ExprRange { start, end, limits, .. }) => {
      let (start, end) = (option(start), option(end));
      let inclusive = matches!(limits, RangeLimits::Closed(_));
      quote!(#root::layout::range::<#ty>(#start, #end, #inclusive))
    }
    value => quote!(((#value) as u128, (#value) as u128)),
  });
  quote!({
    let merged: &'static ([(u128, u128); #count], usize) =
      const { &#root::layout::merge_ranges([#(#ranges),*]) };
    merged.0.split_at(merged.1).0
  })
}

// -------------------------------------------------------------------------------------------------
// Structs

fn derive_struct(input: &DeriveInput, data: &DataStruct) -> Result<TokenStream> {
  let repr = parse_repr(&input.attrs)?;
  let options = parse_options(&input.attrs)?;
  let root = options.root.clone().unwrap_or_else(|| quote!(::transmutant));
  let ident = &input.ident;
  let (impl_generics, ty_generics, _) = input.generics.split_for_impl();
  let this = quote!(#ident #ty_generics);

  let fields = match &data.fields {
    Fields::Named(fields) => Some(&fields.named),
    Fields::Unnamed(fields) => Some(&fields.unnamed),
    Fields::Unit => None,
  };
  let fields = fields
    .into_iter()
    .flatten()
    .enumerate()
    .map(|(i, field)| {
      Ok(FieldInfo {
        member: match &field.ident {
          Some(ident) => Member::Named(ident.clone()),
          None => Member::Unnamed(Index::from(i)),
        },
        ty: &field.ty,
        vis: &field.vis,
        options: parse_options(&field.attrs)?,
      })
    })
    .collect::<Result<Vec<_>>>()?;
  let count = fields.len();

  // Only the layout of `repr(C)` and `repr(transparent)` is known without the size of the fields.
  let sized = !(repr.c || repr.transparent);
  let mut generics = input.generics.clone();
  let predicates = &mut generics.make_where_clause().predicates;
  for field in &fields {
    let ty = field.ty;
    predicates.push(parse_quote!(#ty: #root::Data));
    // What a region is read as.
    for ty in field.options.regions.iter().flatten().filter_map(|region| region.ty.as_ref()) {
      predicates.push(parse_quote!(#ty: #root::Data));
    }
  }
  if sized {
    predicates.push(parse_quote!(#this: ::core::marker::Sized));
  }
  let where_clause = &generics.where_clause;

  // Layout of the fields.
  let pack = repr.packed.unwrap_or_else(|| quote!(::core::primitive::usize::MAX));
  let mut layouts = vec![];
  for (i, field) in fields.iter().enumerate() {
    let (member, ty) = (&field.member, field.ty);
    let offset = match fields.get(i.wrapping_sub(1)) {
      // The last field may be unsized, in which case `offset_of!` does not want it.
      _ if sized || i + 1 < count => quote!(::core::mem::offset_of!(Self, #member)),
      Some(previous) if repr.c => {
        let (previous_member, previous_ty) = (&previous.member, previous.ty);
        quote!(#root::layout::tail_offset(
          ::core::mem::offset_of!(Self, #previous_member) + ::core::mem::size_of::<#previous_ty>(),
          <#ty as #root::Data>::LAYOUT.align(),
          #pack,
        ))
      }
      _ => quote!(0),
    };
    layouts.push(match &field.options.valid {
      Some(ranges) => {
        let ranges = valid_ranges(ranges, ty, &root);
        quote!(#root::layout::Field::restricted(
          #offset,
          const { &<#ty as #root::Data>::LAYOUT.restricted(#ranges) },
        ))
      }
      None => quote!(#root::layout::Field::new(#offset, &<#ty as #root::Data>::LAYOUT)),
    });
  }

  // Rules.
  let rule = quote!(#root::layout::Rule);
  let mut rules = vec![];
  if let Some(size) = &options.size {
    let ops = plain(size, &fields, &root)?;
    rules.push(quote!(#rule::Size(#ops)));
  }
  // The rules that walk: their index, their number of operations and the one of their walk.
  let mut walks = vec![];
  for assert in &options.asserts {
    let Expression { ops, len, walk } = expression(assert, &fields, &root)?;
    rules.push(match walk {
      Some(inner) => {
        walks.push((rules.len(), len, inner));
        quote!(#rule::Walk(#ops))
      }
      None => quote!(#rule::Assert(#ops)),
    });
  }
  let mut accessors = vec![];
  for (i, field) in fields.iter().enumerate() {
    let last = i + 1 == count;
    if let Some(len) = &field.options.len {
      if !last {
        return Err(Error::new(len.span(), "`len` is an option of the last field"));
      }
      let ops = plain(len, &fields, &root)?;
      rules.push(quote!(#rule::Len(#ops)));
    }
    if let Some(regions) = &field.options.regions {
      if !last {
        return Err(Error::new(field.member.span(), "`regions` is an option of the last field"));
      }
      let vis = field.vis;
      for (index, region) in regions.iter().enumerate() {
        let name = &region.name;
        rules.push(match &region.size {
          Some(size) => {
            let ops = plain(size, &fields, &root)?;
            quote!(#rule::Region(::core::option::Option::Some(#ops)))
          }
          None if index + 1 == regions.len() => quote!(#rule::Region(::core::option::Option::None)),
          None => return Err(Error::new(name.span(), "only the last region can be unsized")),
        });
        accessors.push(match &region.ty {
          None => {
            let doc = format!("The bytes of the `{name}` region.");
            quote! {
              #[doc = #doc]
              #[inline]
              #vis const fn #name(&self) -> &[::core::primitive::u8] {
                #root::region(self, #index)
              }
            }
          }
          Some(ty) => {
            let doc = format!("The `{name}` region, if its bytes are what it is said to hold.");
            quote! {
              #[doc = #doc]
              #[inline]
              #vis const fn #name(&self) -> ::core::result::Result<&#ty, #root::Error> {
                #root::try_transmute_ref::<[::core::primitive::u8], #ty>(#root::region(self, #index))
              }
            }
          }
        });
      }
    }
  }

  // Getters.
  for getter in &options.getters {
    let (name, ty, vis) = (&getter.name, &getter.ty, &input.vis);
    let expr = rust(&getter.expr, &fields, &root);
    let doc = format!("`{}`", getter.expr.to_token_stream());
    accessors.push(quote! {
      #[doc = #doc]
      #[inline]
      #[allow(unused_parens, clippy::all)]
      #vis const fn #name(&self) -> #ty {
        #expr as #ty
      }
    });
  }

  // The tags the type goes with.
  let tags = options.tags.iter().map(|TagOption { key, tag }| {
    quote! {
      impl #impl_generics #root::Tag<#key> for #this #where_clause {
        type Tag = #tag;
      }
    }
  });

  // The probe: a null pointer with the slice length of the probe of the last field, if any.
  let probe = match fields.last() {
    Some(last) if !sized => {
      let ty = last.ty;
      quote!(<#ty as #root::Data>::PROBE as *const Self)
    }
    _ => quote!(::core::ptr::null()),
  };

  // The list of the fields, at the type level.
  let markers = (0..count).map(|i| format_ident!("__DataField{}", i)).collect::<Vec<_>>();
  let first = match markers.first() {
    Some(marker) => quote!(#marker #ty_generics),
    None => quote!(()),
  };
  let vis = &input.vis;
  let lists = fields.iter().enumerate().map(|(i, field)| {
    let (marker, ty) = (&markers[i], field.ty);
    let next = match markers.get(i + 1) {
      Some(marker) => quote!(#marker #ty_generics),
      None => quote!(()),
    };
    quote! {
      // As visible as the struct, so that it can be named by its implementation of `Data`.
      #[allow(non_camel_case_types)]
      #vis struct #marker #impl_generics(::core::marker::PhantomData<#this>) #where_clause;

      // SAFETY: the field of the layout of the struct.
      unsafe impl #impl_generics #root::Fields for #marker #ty_generics #where_clause {
        type Type = #ty;
        type Next = #next;
        type Path<__P: #root::Path> = #root::At<__P, Self>;
        const FIELD: ::core::option::Option<#root::layout::Field> =
          ::core::option::Option::Some(*<#this as #root::Data>::LAYOUT.field(#i));
      }
    }
  });

  // The list of the rules that walk, at the type level.
  let walk_markers = (0..walks.len()).map(|i| format_ident!("__DataWalk{}", i)).collect::<Vec<_>>();
  let first_walk = match walk_markers.first() {
    Some(marker) => quote!(#marker #ty_generics),
    None => quote!(()),
  };
  let walk_lists = walks.iter().enumerate().map(|(i, (index, len, inner))| {
    let (marker, ops, inner) = (&walk_markers[i], count_type(*len, &root), count_type(*inner, &root));
    let next = match walk_markers.get(i + 1) {
      Some(marker) => quote!(#marker #ty_generics),
      None => quote!(()),
    };
    quote! {
      #[allow(non_camel_case_types)]
      #vis struct #marker #impl_generics(::core::marker::PhantomData<#this>) #where_clause;

      // SAFETY: the rule of the layout of the struct, and the sizes of its expressions.
      unsafe impl #impl_generics #root::Walks for #marker #ty_generics #where_clause {
        const RULE: ::core::primitive::usize = #index;
        type Ops = #ops;
        type Inner = #inner;
        type Next = #next;
      }
    }
  });

  let accessors = (!accessors.is_empty()).then(|| {
    quote! {
      impl #impl_generics #this #where_clause {
        #(#accessors)*
      }
    }
  });

  Ok(quote! {
    const _: () = {
      #(#lists)*

      #(#walk_lists)*

      // SAFETY: the layout comes from the compiler, field by field.
      unsafe impl #impl_generics #root::Data for #this #where_clause {
        type Elem = ();
        type Fields = #first;
        type Variants = ();
        type Walks = #first_walk;
        const PROBE: *const Self = #probe;
        // SAFETY: if the struct is unsized, its probe is the one of its last field, whose tail
        // is a slice.
        const LAYOUT: #root::layout::Layout = unsafe {
          #root::layout::Layout::structure::<Self>(
            Self::PROBE,
            const { &#root::layout::Layout::sort_fields([#(#layouts),*]) },
            const { &[#(#rules),*] },
          )
        };
      }

      #accessors

      #(#tags)*
    };
  })
}

// -------------------------------------------------------------------------------------------------
// Enums

fn derive_enum(input: &DeriveInput, data: &DataEnum) -> Result<TokenStream> {
  let options = parse_options(&input.attrs)?;
  let root = options.root.unwrap_or_else(|| quote!(::transmutant));
  let ident = &input.ident;
  let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();
  if data.variants.is_empty() {
    return Err(Error::new(Span::call_site(), "an enum without variant has no value"));
  }
  // An enum of references: a view of what is one thing or another.
  let references = data
    .variants
    .iter()
    .filter_map(|variant| match &variant.fields {
      Fields::Unnamed(fields) if fields.unnamed.len() == 1 => match &fields.unnamed[0].ty {
        Type::Reference(reference) if reference.mutability.is_none() => {
          Some((&variant.ident, reference))
        }
        _ => None,
      },
      _ => None,
    })
    .collect::<Vec<_>>();
  if references.len() == data.variants.len() {
    let lifetime = &references[0].1.lifetime;
    if lifetime.is_none() || references.iter().any(|(_, reference)| &reference.lifetime != lifetime) {
      return Err(Error::new(Span::call_site(), "the references of the variants must have the same lifetime"));
    }
    let (vis, bytes) = (&input.vis, quote!(&#lifetime [::core::primitive::u8]));
    let result = quote!(::core::result::Result);
    let unpacks = references.iter().map(|(variant, reference)| {
      let ty = &reference.elem;
      quote! {
        if let #result::Ok((value, rest)) = #root::unpack::<#ty>(bytes) {
          return #result::Ok((Self::#variant(value), rest));
        }
      }
    });
    let parses = references.iter().map(|(variant, reference)| {
      let ty = &reference.elem;
      quote! {
        if let #result::Ok(value) = #root::try_transmute_ref::<[::core::primitive::u8], #ty>(bytes) {
          return #result::Ok(Self::#variant(value));
        }
      }
    });
    return Ok(quote! {
      impl #impl_generics #ident #ty_generics #where_clause {
        /// Take the first of the variants that `bytes` start with: returns it and what remains.
        #vis const fn unpack(bytes: #bytes) -> #result<(Self, #bytes), #root::Error> {
          #(#unpacks)*
          #result::Err(#root::Error::Validity)
        }

        /// The first of the variants that `bytes` are, whole.
        #vis const fn parse(bytes: #bytes) -> #result<Self, #root::Error> {
          #(#parses)*
          #result::Err(#root::Error::Validity)
        }
      }
    });
  }
  if data.variants.iter().any(|variant| !matches!(variant.fields, Fields::Unit)) {
    return derive_tagged(input, data, &root);
  }
  let mut variants = vec![];
  for variant in &data.variants {
    let variant = &variant.ident;
    // SAFETY (of the generated code): an enum without fields has no padding.
    variants.push(quote!(unsafe { #root::layout::bits_of(&Self::#variant) }));
  }
  let count = variants.len();
  Ok(quote! {
    // SAFETY: the valid values are the ones of the variants, read from the compiler.
    unsafe impl #impl_generics #root::Scalar for #ident #ty_generics #where_clause {
      const SPEC: #root::layout::Spec = if ::core::mem::size_of::<Self>() == 0 {
        #root::layout::Spec::any::<Self>()
      } else {
        let merged: &'static ([(u128, u128); #count], usize) =
          const { &#root::layout::sorted_ranges([#(#variants),*]) };
        #root::layout::Spec::ranges::<Self>(
          #root::layout::Endian::NATIVE,
          merged.0.split_at(merged.1).0,
        )
      };
    }
  })
}

/// An enum with fields, of a specified layout (RFC 2195): the offsets of the fields are the ones
/// of `repr(C)` structs, which the compiler is asked for.
fn derive_tagged(input: &DeriveInput, data: &DataEnum, root: &TokenStream) -> Result<TokenStream> {
  let repr = parse_repr(&input.attrs)?;
  if !repr.c && repr.int.is_none() {
    return Err(Error::new(
      input.ident.span(),
      "the layout of an enum with fields is not specified: `Data` can be derived for it with \
       `#[repr(u8)]` (or another integer), `#[repr(C)]`, or both",
    ));
  }
  let (ident, vis) = (&input.ident, &input.vis);
  let (impl_generics, ty_generics, _) = input.generics.split_for_impl();
  let this = quote!(#ident #ty_generics);
  let mut generics = input.generics.clone();
  let predicates = &mut generics.make_where_clause().predicates;
  for field in data.variants.iter().flat_map(|variant| &variant.fields) {
    let ty = &field.ty;
    predicates.push(parse_quote!(#ty: #root::Data));
  }
  let where_clause = &generics.where_clause;

  // The tag: an enum without fields of the same variants.
  let tag = format_ident!("__DataTag");
  let tag_repr = match &repr.int {
    Some(int) => quote!(#int),
    None => quote!(C),
  };
  let tags = data.variants.iter().map(|variant| {
    let (name, discriminant) = (&variant.ident, variant.discriminant.as_ref().map(|(_, expr)| expr));
    let discriminant = discriminant.map(|expr| quote!(= #expr));
    quote!(#name #discriminant)
  });
  let names = data.variants.iter().map(|variant| &variant.ident).collect::<Vec<_>>();
  let count = names.len();

  // A `repr(C)` struct for each variant, which starts with the tag unless the enum is `repr(C)`.
  let shadows = (0..count).map(|i| format_ident!("__DataVariant{}", i)).collect::<Vec<_>>();
  let first = usize::from(!repr.c);
  let sizes = shadows.iter().map(|shadow| {
    quote!((
      ::core::mem::size_of::<#shadow #ty_generics>(),
      ::core::mem::align_of::<#shadow #ty_generics>(),
    ))
  });
  let sizes = quote!([#(#sizes),*]);
  let tag_layout = quote!(<#tag as #root::Data>::LAYOUT);
  // Where the fields of the variants are.
  let fields = match repr.c {
    true => quote!(#root::layout::Enum::fields(&#tag_layout, &#sizes)),
    false => quote!(0),
  };
  let (mut items, mut variants) = (vec![], vec![]);
  for (i, variant) in data.variants.iter().enumerate() {
    let (shadow, name) = (&shadows[i], &names[i]);
    let types = variant.fields.iter().map(|field| &field.ty).collect::<Vec<_>>();
    let head = (!repr.c).then(|| quote!(#tag,));
    let markers =
      (0..types.len()).map(|j| format_ident!("__DataVariant{}Field{}", i, j)).collect::<Vec<_>>();
    let mut layouts = vec![];
    for (j, field) in variant.fields.iter().enumerate() {
      let options = parse_options(&field.attrs)?;
      if options.len.is_some() || options.regions.is_some() {
        return Err(Error::new(field.span(), "the fields of an enum are sized"));
      }
      let (ty, index) = (&field.ty, Index::from(first + j));
      let offset = quote!(#fields + ::core::mem::offset_of!(#shadow #ty_generics, #index));
      layouts.push(match &options.valid {
        Some(ranges) => {
          let ranges = valid_ranges(ranges, ty, root);
          quote!(#root::layout::Field::restricted(
            #offset,
            const { &<#ty as #root::Data>::LAYOUT.restricted(#ranges) },
          ))
        }
        None => quote!(#root::layout::Field::new(#offset, &<#ty as #root::Data>::LAYOUT)),
      });
      let (marker, next) = (&markers[j], match markers.get(j + 1) {
        Some(marker) => quote!(#marker #ty_generics),
        None => quote!(()),
      });
      items.push(quote! {
        #[allow(non_camel_case_types)]
        #vis struct #marker #impl_generics(::core::marker::PhantomData<#this>) #where_clause;

        // SAFETY: the field of the layout of the variant.
        unsafe impl #impl_generics #root::Fields for #marker #ty_generics #where_clause {
          type Type = #ty;
          type Next = #next;
          type Path<__P: #root::Path> = #root::At<__P, Self>;
          const FIELD: ::core::option::Option<#root::layout::Field> =
            ::core::option::Option::Some(*<#this as #root::Data>::LAYOUT.variant_field(#i, #j));
        }
      });
    }
    let (fields, next) = (
      match markers.first() {
        Some(marker) => quote!(#marker #ty_generics),
        None => quote!(()),
      },
      match shadows.get(i + 1) {
        Some(shadow) => quote!(#shadow #ty_generics),
        None => quote!(()),
      },
    );
    items.push(quote! {
      // As visible as the enum, so that it can be named by its implementation of `Data`.
      #[repr(C)]
      #[allow(dead_code, non_camel_case_types)]
      #vis struct #shadow #impl_generics(
        #head #(#types,)* ::core::marker::PhantomData<#this>
      ) #where_clause;

      // SAFETY: the variant of the layout of the enum.
      unsafe impl #impl_generics #root::Variants for #shadow #ty_generics #where_clause {
        type Fields = #fields;
        type Next = #next;
        const TAG: ::core::option::Option<::core::primitive::u128> =
          ::core::option::Option::Some(<#this as #root::Data>::LAYOUT.variant_tag(#i));
      }
    });
    variants.push(quote!(#root::layout::Variant::new(
      // SAFETY (of the generated code): an enum without fields has no padding.
      unsafe { #root::layout::bits_of(&#tag::#name) },
      const {
        &#root::layout::Layout::variant::<Self>(
          const { &#root::layout::Layout::sort_fields([#(#layouts),*]) },
        )
      },
    )));
  }
  let (first, outside) = (&shadows[0], repr.c);

  Ok(quote! {
    const _: () = {
      #[repr(#tag_repr)]
      #[derive(Clone, Copy)]
      #[allow(dead_code, non_camel_case_types, clippy::enum_variant_names)]
      enum #tag { #(#tags),* }

      // SAFETY: the valid values are the ones of the variants, read from the compiler.
      unsafe impl #root::Scalar for #tag {
        const SPEC: #root::layout::Spec = {
          let merged: &'static ([(u128, u128); #count], usize) = const {
            // SAFETY: an enum without fields has no padding.
            &#root::layout::sorted_ranges([#(unsafe { #root::layout::bits_of(&#tag::#names) }),*])
          };
          #root::layout::Spec::ranges::<Self>(
            #root::layout::Endian::NATIVE,
            merged.0.split_at(merged.1).0,
          )
        };
      }

      #(#items)*

      // SAFETY: the layout is the one that RFC 2195 specifies, made from what the compiler tells
      // of a `repr(C)` struct for each variant; it is checked against the size and the alignment
      // of the enum.
      unsafe impl #impl_generics #root::Data for #this #where_clause {
        type Elem = ();
        type Fields = ();
        type Variants = #first #ty_generics;
        type Walks = ();
        const PROBE: *const Self = ::core::ptr::null();
        const LAYOUT: #root::layout::Layout = #root::layout::Layout::enumeration::<Self>(
          const {
            &#root::layout::Enum::new(
              &#tag_layout,
              #outside,
              &#sizes,
              #fields,
              const { &[#(#variants),*] },
            )
          },
        );
      }
    };
  })
}
