use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::{
  parenthesized, parse_macro_input, punctuated::Punctuated, token, Attribute, Data, DeriveInput,
  Error, Expr, Fields, Index, Path, Token,
};

macro_rules! bail {
  ($msg:expr $(,)?) => {
    return Error::new(Span::call_site(), &$msg[..]).to_compile_error().into()
  };
  ($msg:expr => $span_to_blame:expr $(,)?) => {
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

fn parse_derive_attr(attrs: &[Attribute]) -> (bool, TokenStream) {
  let mut repr_c_or_transparent = false;
  let mut root = quote!(::mutant);
  for attr in attrs {
    if attr.path().is_ident("repr") {
      attr
        .parse_nested_meta(|meta| {
          // #[repr(C)]
          if meta.path.is_ident("C") {
            repr_c_or_transparent = true;
          }
          // #[repr(transparent)]
          if meta.path.is_ident("transparent") {
            repr_c_or_transparent = true;
          }
          Ok(())
        })
        .unwrap();
    }
    if attr.path().is_ident("data") {
      attr
        .parse_nested_meta(|meta| {
          // #[data(crate(<Path>))]
          if meta.path.is_ident("crate") {
            if meta.input.peek(token::Paren) {
              let content;
              parenthesized!(content in meta.input);
              let path: Path = content.parse()?;
              root = quote!(#path);
            } else {
              root = quote!(crate);
            }
            return Ok(());
          }

          Err(meta.error("unrecognized data attribute"))
        })
        .unwrap();
    }
  }
  (repr_c_or_transparent, root)
}

fn parse_field_attr(attrs: &[Attribute]) -> (TokenStream, TokenStream) {
  let mut factor = quote!();
  let mut expect = quote!();
  for attr in attrs {
    if !attr.path().is_ident("data") {
      continue;
    }
    attr
      .parse_nested_meta(|meta| {
        // #[data(expect(<Expr>))]
        if meta.path.is_ident("expect") {
          let content;
          parenthesized!(content in meta.input);
          let expr: Expr = content.parse()?;
          expect = quote!(.with_expect(#expr));
          return Ok(());
        }

        // #[data(size)] or #[data(size(N))], omitted N means 1
        if meta.path.is_ident("size") {
          if meta.input.peek(token::Paren) {
            let content;
            parenthesized!(content in meta.input);
            let expr: Expr = content.parse()?;
            factor = quote!(.with_factor(::core::option::Option::Some(#expr)));
          } else {
            factor = quote!(.with_factor(::core::option::Option::Some(1)));
          }
          return Ok(());
        }

        Err(meta.error("unrecognized data attribute"))
      })
      .unwrap();
  }
  (factor, expect)
}

#[proc_macro_derive(Data, attributes(data))]
pub fn derive_data(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
  let input = parse_macro_input!(input as DeriveInput);
  let (repr_c_or_transparent, root) = parse_derive_attr(&input.attrs);

  let data = match input.data {
    Data::Struct(data) => data,
    _ => bail!("#[derive(Data)] is yet only implemented for struct"),
  };

  if !repr_c_or_transparent {
    bail!("`#[repr(C)]` or `#[repr(transparant)]` is required in stable rust")
  }

  // Compute generics from input.
  let generics = input.generics;
  let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

  // Compute identifier from input.
  let ident = input.ident;

  // Compute fields from input.
  let input_fields = match &data.fields {
    Fields::Named(fields) if !fields.named.is_empty() => &fields.named,
    Fields::Unnamed(fields) if !fields.unnamed.is_empty() => &fields.unnamed,
    // Empty struct, impl as `Layout::Void`.
    _ => {
      return proc_macro::TokenStream::from(quote! {
        unsafe impl #impl_generics #root::Data for #ident #ty_generics #where_clause {
          type Item = ();
          type Fields = ();

          const LAYOUT: #root::Layout<Self> = #root::Layout::Void;
          const HEADER: usize = 0;
        }
      })
    }
  };

  // Compute head.
  let tail = match input_fields.last() {
    Some(field) => field,
    _ => unreachable!(),
  };
  let tail_ty = &tail.ty;
  let tail_ident = match &tail.ident {
    Some(ident) => quote!(#ident),
    _ => {
      let ident = Index { index: (input_fields.len() - 1) as u32, span: Span::call_site() };
      quote!(#ident)
    }
  };

  // Compute `where` clause.
  let predicates = input_fields
    .iter()
    .take(input_fields.len() - 1)
    .map(|field| &field.ty)
    .map(|ty| quote!(#ty: #root::Data + Copy))
    .chain([quote!(#tail_ty: #root::Data)]);
  let where_clause = match where_clause {
    None => quote!(where #(#predicates,)*),
    Some(where_clause) => {
      let existing_predicates = where_clause.predicates.iter();
      quote!(where #(#existing_predicates,)* #(#predicates,)*)
    }
  };

  // Compute head fields.
  let fields_tys = input_fields.iter().map(|field| &field.ty);
  let fields_ty =
    fields_tys.clone().rev().fold(quote!(()), |tail, ty| quote!((#root::Field<Self, #ty>, #tail)));
  let fields = input_fields
    .iter()
    .map(|field| (field.ident.as_ref(), &field.ty, parse_field_attr(&field.attrs)))
    .enumerate()
    .map(|(i, (ident, ty, (factor, expect)))| {
      let ident = match ident {
        Some(ident) => quote!(#ident),
        _ => {
          let ident = Index { index: i as u32, span: Span::call_site() };
          quote!(#ident)
        }
      };
      quote!(unsafe {
        #root::field::<Self, #ty>(#root::offset_of!(Self, #ident)) #factor #expect
      })
    })
    .collect::<Vec<_>>()
    .iter()
    .rev()
    .fold(quote!(()), |tail, head| quote!((#head, #tail)));

  proc_macro::TokenStream::from(quote!(
    unsafe impl #impl_generics #root::Data for #ident #ty_generics #where_clause {
      type Item = ();
      type Fields = #fields_ty;

      const LAYOUT: #root::Layout<Self> = #root::Layout::Struct { repr: #root::Repr::new(#fields) };
      const HEADER: usize = {
        let size = #root::offset_of!(Self, #tail_ident) + #root::min_size_of::<#tail_ty>();
        if !#root::is_sized::<#tail_ty>() {
          size
        } else {
          let align = #root::align_of::<Self>();
          size.wrapping_add(align).wrapping_sub(1) & !align.wrapping_sub(1)
        }
      };
    }
  ))
}
