use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote};
use syn::{parse_macro_input, parenthesized, token, punctuated::Punctuated, Data, DeriveInput, Error, Expr, Fields, Index, Token, Attribute, Path, parse_quote, Type};

macro_rules! bail {
  ($msg:expr $(,)?) => {
    return Error::new(Span::call_site(), &$msg[..]).to_compile_error().into()
  };
  ( $msg:expr => $span_to_blame:expr $(,)? ) => {
    return Error::new_spanned(&$span_to_blame, $msg).to_compile_error().into()
  };
}

#[proc_macro]
pub fn ensure(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
  let args = input.clone();
  let mut args: Vec<Expr> =
    parse_macro_input!(args with Punctuated::<Expr, Token![,]>::parse_terminated)
      .into_iter()
      .collect();

  let input = proc_macro2::TokenStream::from(input);
  let cond = match args.is_empty() {
    false => args.remove(0),
    _ => bail!("`attest` requires at least one expression argument."),
  };
  let asm = match args.is_empty() {
    true => quote!("\r[\x08ERROR     attest failed"),
    _ => quote!(::core::concat!("\r[\x08ERROR     ", #(#args),*)),
  };
  quote!(
    #[cfg(debug_assertions)]
    { assert!(#input); }
    #[cfg(not(debug_assertions))]
    if !#cond {
      unsafe {
        ::core::arch::asm!(#asm);
        ::core::hint::unreachable_unchecked();
      }
    }
  )
  .into()
}

fn parse_derive_attr(attrs: &[Attribute]) -> TokenStream {
  let mut crate_name = quote!(::mutant);
  for attr in attrs {
    if !attr.path().is_ident("mutant") {
      continue;
    }
    attr.parse_nested_meta(|meta| {
      // #[mutant(crate(<Path>))]
      if meta.path.is_ident("crate") {
        if meta.input.peek(token::Paren) {
          let content;
          parenthesized!(content in meta.input);
          let path: Path = content.parse()?;
          crate_name = quote!(#path);
        } else {
          crate_name = quote!(crate);
        }
        return Ok(());
      }

      Err(meta.error("unrecognized mutant attribute"))
    }).unwrap();
  }
  crate_name
}

fn parse_field_attr(attrs: &[Attribute]) -> (TokenStream, TokenStream) {
  let mut factor = quote!(::core::option::Option::None);
  let mut expect = quote!(::core::option::Option::None);
  for attr in attrs {
    if !attr.path().is_ident("mutant") {
      continue;
    }
    attr.parse_nested_meta(|meta| {
      // #[mutant(expect(<Expr>))]
      if meta.path.is_ident("expect") {
        let content;
        parenthesized!(content in meta.input);
        let expr: Expr = content.parse()?;
        expect = quote!(#expr);
        return Ok(());
      }

      // #[mutant(size)] or #[mutant(size(N))], omitted N means 1
      if meta.path.is_ident("size") {
        if meta.input.peek(token::Paren) {
          let content;
          parenthesized!(content in meta.input);
          let expr: Expr = content.parse()?;
          factor = quote!(::core::option::Option::Some(#expr));
        } else {
          factor = quote!(::core::option::Option::Some(1));
        }
        return Ok(());
      }

      Err(meta.error("unrecognized mutant attribute"))
    }).unwrap();
  }
  (factor, expect)
}

#[proc_macro_derive(DataType, attributes(mutant))]
pub fn derive_datatype(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
  let input = parse_macro_input!(input as DeriveInput);
  let root = parse_derive_attr(&input.attrs);

  let data = match input.data {
    Data::Struct(data) => data,
    _ => bail!("#[derive(DataType)] is yet only implemented for struct"),
  };

  // Compute `#[repr(..)]` attribute(s), visibility and generics from input.
  let repr = input.attrs.iter().filter_map(|attr| attr.path().is_ident("repr").then_some(attr));
  let vis = input.vis;
  let generics = input.generics;
  let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

  // Compute identifiers from input.
  let head_ident = format_ident!("__{}Head", input.ident);
  let ident = input.ident;

  // Compute fields from input.
  let input_fields = match data.fields {
    Fields::Named(fields) => fields.named,
    Fields::Unnamed(fields) => fields.unnamed,
    // Empty struct, impl as `Layout::Void`.
    Fields::Unit => return proc_macro::TokenStream::from(quote! {
      unsafe impl #impl_generics #root::DataType for #ident #ty_generics #where_clause {
        type Item = ();
        type Head = Self;
        type Tail = ();
        type Fields = ();

        const LAYOUT: #root::Layout<Self> = #root::Layout::Void;
      }
    })
  };

  // Compute head & tail.
  let tail = match input_fields.last() { Some(field) => field, _ => bail!("empty struct") };
  let tail_ty = &tail.ty;
  let tail_head_ty: Type = parse_quote!(<#tail_ty as #root::DataType>::Head);
  let head = input_fields.iter().take(input_fields.len() - 1);

  // Compute head fields.
  let fields_tys = head.clone().map(|field| &field.ty).chain([&tail_head_ty]);
  let fields_ty = fields_tys
    .clone()
    .rev()
    .fold(quote!(()), |tail, ty| quote!((#root::Field<Self, #ty>, #tail)));
  let fields = head
    .clone()
    .map(|field| (&field.ty, parse_field_attr(&field.attrs)))
    .chain([(&tail_head_ty, parse_field_attr(&tail.attrs))])
    .enumerate()
    .map(|(i, (ty, (factor, expect)))| {
      let ident = Index { index: i as u32, span: Span::call_site() };
      quote!(unsafe {
        #root::field::<Self, #ty>(::core::mem::offset_of!(Self::Head, #ident), #factor, #expect)
      })
    })
    .collect::<Vec<_>>()
    .iter()
    .rev()
    .fold(quote!(()), |tail, head| quote!((#head, #tail)));

  let predicates = head.clone()
    .map(|field| &field.ty)
    .map(|ty| quote!(#ty: #root::DataType + Copy));
  let where_clause = match where_clause {
    None => quote!(where #(#predicates,)* #tail_ty: #root::DataType),
    Some(where_clause) => {
      let existing_predicates = where_clause.predicates.iter();
      quote!(where #(#existing_predicates,)* #(#predicates,)* #tail_ty: #root::DataType)
    }
  };

  proc_macro::TokenStream::from(quote!(
    #(#repr)* #vis struct #head_ident #impl_generics (#(#fields_tys,)*) #where_clause;

    impl #impl_generics Copy for #head_ident #ty_generics #where_clause { }
    impl #impl_generics Clone for #head_ident #ty_generics #where_clause {
      fn clone(&self) -> Self {
        *self
      }
    }

    impl #impl_generics #root::DataType for #head_ident #ty_generics #where_clause {
      type Item = ();
      type Head = Self;
      type Tail = ();
      type Fields = #fields_ty;

      const LAYOUT: #root::Layout<Self> = #root::Layout::Struct { repr: #root::Repr::new(#fields) };
    }

    impl #impl_generics #root::DataType for #ident #ty_generics #where_clause {
      type Item = ();
      type Head = #head_ident #ty_generics;
      type Tail = <#tail_ty as #root::DataType>::Tail;
      type Fields = #fields_ty;

      const LAYOUT: #root::Layout<Self> = #root::Layout::Struct { repr: #root::Repr::new(#fields) };
    }
  ))
}
