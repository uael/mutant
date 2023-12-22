use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote, ToTokens};
use syn::{
  parse_macro_input, punctuated::Punctuated, Data, DeriveInput, Error, Expr, Fields, Index, Token,
};

macro_rules! bail_tokens {
  ($msg:expr $(,)?) => {
    return Error::new(Span::call_site(), &$msg[..]).to_compile_error().into()
  };
  ( $msg:expr => $span_to_blame:expr $(,)? ) => {
    return Error::new_spanned(&$span_to_blame, $msg).to_compile_error().into()
  };
}

macro_rules! bail {
  ($msg:expr $(,)?) => {
    return Err(Error::new(Span::call_site(), &$msg[..]))
  };
  ( $msg:expr => $span_to_blame:expr $(,)? ) => {
    return Err(Error::new_spanned(&$span_to_blame, $msg))
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
    _ => bail_tokens!("`attest` requires at least one expression argument."),
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

fn parse_mutant_attr(input: &DeriveInput) -> (TokenStream, TokenStream, TokenStream, TokenStream) {
  const CRATE_ATTR: &'static str = "crate";
  const TAG_ATTR: &'static str = "tag";
  const TAG_SPEC_ATTR: &'static str = "tag_spec";
  const SIZE_SPEC_ATTR: &'static str = "size_spec";

  let mut crate_name = quote!(::mutant);
  let mut tag = quote!(::core::option::Option::None);
  let mut tag_spec = quote!(::core::option::Option::None);
  let mut size_spec = quote!(::core::option::Option::None);

  for attr in &input.attrs {
    if !attr.path().is_ident("mutant") {
      continue;
    }

    attr
      .parse_nested_meta(|meta| {
        if meta.path.is_ident(TAG_ATTR) {
          let expr: syn::Expr = meta.value()?.parse()?;
          tag = quote!(::core::option::Option::Some(#expr));
        }
        if meta.path.is_ident(TAG_SPEC_ATTR) {
          let expr: syn::Expr = meta.value()?.parse()?;
          tag_spec = quote!(::core::option::Option::Some(#expr));
        }
        if meta.path.is_ident(SIZE_SPEC_ATTR) {
          let expr: syn::Expr = meta.value()?.parse()?;
          size_spec = quote!(::core::option::Option::Some(#expr));
        }
        if meta.path.is_ident(CRATE_ATTR) {
          let expr: syn::Expr = meta.value()?.parse()?;
          let mut value = &expr;
          while let syn::Expr::Group(e) = value {
            value = &e.expr;
          }
          if let syn::Expr::Lit(syn::ExprLit { lit: syn::Lit::Str(lit), .. }) = value {
            let suffix = lit.suffix();
            if !suffix.is_empty() {
              bail!(format!("Unexpected suffix `{}` on string literal", suffix))
            }
            let path: syn::Path = match lit.parse() {
              Ok(path) => path,
              Err(_) => {
                bail!(format!("Failed to parse path: {:?}", lit.value()))
              }
            };
            crate_name = path.into_token_stream();
          } else {
            bail!("Expected mutant `crate` attribute to be a string: `crate = \"...\"`")
          }
        }
        Ok(())
      })
      .unwrap();
  }

  (crate_name, tag, tag_spec, size_spec)
}

#[proc_macro_derive(Mutant, attributes(mutant))]
pub fn derive_mutant(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
  let input = parse_macro_input!(input as DeriveInput);
  let (mutant, tag, tag_spec, size_spec) = parse_mutant_attr(&input);

  let data = match input.data {
    Data::Struct(data) => data,
    _ => bail_tokens!("#[derive(Mutant)] is yet only implemented for struct"),
  };

  let repr = input
    .attrs
    .iter()
    .filter_map(|attr| attr.path().is_ident("repr").then_some(attr))
    .collect::<Vec<_>>();

  let vis = input.vis;
  let generics = input.generics;
  let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

  let head = format_ident!("__{}Head", input.ident);
  let this = input.ident;

  let fields = match data.fields {
    Fields::Unit => {
      return quote! {
        unsafe impl #impl_generics #mutant::Mutant for #this #ty_generics #where_clause {
          type Head = ();
          type Tail = ();
          type Item = ();

          const HEAD: #mutant::TypeLayout<Self::Head> = <()>::HEAD;
          const TAIL: #mutant::TypeLayout<Self::Item> = <()>::TAIL;

          const TAG: ::core::option::Option<u64> = #tag;
        }
      }
      .into()
    }
    Fields::Named(fields) => fields.named,
    Fields::Unnamed(fields) => fields.unnamed,
  };

  let last_ty = match fields.last() {
    Some(field) => &field.ty,
    None => bail_tokens!("empty struct"),
  };

  let head_tys =
    fields.iter().map(|field| &field.ty).map(|ty| quote!(<#ty as #mutant::Mutant>::Head));
  let head_inits = fields.iter().enumerate().map(|(i, field)| {
    let field_name = Index { index: i as u32, span: Span::call_site() };
    let field_ty = &field.ty;
    quote!(layout.#field_name = #mutant::layout_of::<#field_ty>())
  });

  let predicates = fields
    .iter()
    .map(|field| &field.ty)
    .take(fields.len() - 1)
    .map(|ty| quote!(#ty: #mutant::Mutant + Copy));
  let where_clause = match where_clause {
    None => quote!(where #(#predicates,)* #last_ty: #mutant::Mutant),
    Some(where_clause) => {
      let existing_predicates = where_clause.predicates.iter();
      quote!(where #(#existing_predicates,)* #(#predicates,)* #last_ty: #mutant::Mutant)
    }
  };

  quote!(
    #(#repr)*
    #vis struct #head #impl_generics (
      #(#head_tys,)*
      [<#last_ty as #mutant::Mutant>::Item; 0]
    ) #where_clause;

    impl #impl_generics Copy for #head #ty_generics #where_clause { }

    impl #impl_generics Clone for #head #ty_generics #where_clause {
      fn clone(&self) -> Self {
        *self
      }
    }

    unsafe impl #impl_generics #mutant::Mutant for #this #ty_generics #where_clause {
      type Head = #head #ty_generics;
      type Tail = #last_ty;
      type Item = <#last_ty as #mutant::Mutant>::Item;

      const HEAD: #mutant::TypeLayout<Self::Head> = unsafe {
        let mut layout: Self::Head = ::core::mem::zeroed();
        #(#head_inits;)*
        #mutant::TypeLayout::new(layout)
      };
      const TAIL: #mutant::TypeLayout<Self::Item> = <#last_ty as #mutant::Mutant>::TAIL;

      const TAG: ::core::option::Option<u64> = #tag;
      const TAG_SPEC: ::core::option::Option<#mutant::IntSpecifier> = #tag_spec;
      const SIZE_SPEC: ::core::option::Option<#mutant::IntSpecifier> = #size_spec;
    }
  )
  .into()
}
