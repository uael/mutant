use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote};
use syn::{
  parenthesized, parse_macro_input, token, Attribute, Data, DeriveInput, Error, Fields, Index, Path,
};

macro_rules! bail {
  ($msg:expr $(,)?) => {
    return Error::new(Span::call_site(), &$msg[..]).to_compile_error().into()
  };
  ($msg:expr => $span_to_blame:expr $(,)?) => {
    return Error::new_spanned(&$span_to_blame, $msg).to_compile_error().into()
  };
}

#[derive(Copy, Clone)]
enum Repr {
  Rust,
  C,
  Transparant,
}

fn parse_derive_attr(attrs: &[Attribute]) -> (Repr, TokenStream) {
  let mut repr = Repr::Rust;
  let mut root = quote!(::mutant);
  for attr in attrs {
    if attr.path().is_ident("repr") {
      attr
        .parse_nested_meta(|meta| {
          // #[repr(C)]
          if meta.path.is_ident("C") {
            repr = Repr::C;
          }
          // #[repr(transparent)]
          if meta.path.is_ident("transparent") {
            repr = Repr::Transparant;
          }
          // #[repr(Rust)]
          if meta.path.is_ident("Rust") {
            repr = Repr::Rust;
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
  (repr, root)
}

#[proc_macro_derive(Data, attributes(data))]
pub fn derive_data(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
  let input = parse_macro_input!(input as DeriveInput);
  let (repr, root) = parse_derive_attr(&input.attrs);

  let data = match input.data {
    Data::Struct(data) => data,
    _ => bail!("#[derive(Data)] is yet only implemented for struct"),
  };

  // Compute generics from input.
  let vis = input.vis;
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
          type Tail = ();
          type Repr = ();

          const LAYOUT: #root::Layout = #root::Layout::Void;
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
  let predicates = match repr {
    Repr::Rust => input_fields
      .iter()
      .map(|field| &field.ty)
      .map(|ty| quote!(#ty: #root::Data + Copy))
      .chain([quote!(Self: Sized)])
      .collect::<Vec<_>>(),
    Repr::C | Repr::Transparant => input_fields
      .iter()
      .take(input_fields.len() - 1)
      .map(|field| &field.ty)
      .map(|ty| quote!(#ty: #root::Data + Copy))
      .chain([quote!(#tail_ty: #root::Data)])
      .collect::<Vec<_>>(),
  };
  let where_clause = match where_clause {
    None => quote!(where #(#predicates),*),
    Some(where_clause) => {
      let existing_predicates = where_clause.predicates.iter();
      quote!(where #(#existing_predicates,)* #(#predicates),*)
    }
  };

  // Compute head fields.
  let mut impl_fields = vec![];
  let fields = input_fields.iter().enumerate().rfold(quote!(()), |tail, (i, head)| {
    let ty = &head.ty;
    let field_ident = match &head.ident {
      Some(ident) => quote!(#ident),
      _ => {
        let ident = Index { index: i as u32, span: Span::call_site() };
        quote!(#ident)
      }
    };
    let struct_ident = format_ident!("__{}{}", ident.to_string(), i);
    let fields = input_fields.len() - i;
    impl_fields.push(quote! {
      #[allow(non_camel_case_types)]
      #vis struct #struct_ident;

      unsafe impl #impl_generics #root::Repr<#ident #ty_generics> for #struct_ident #where_clause {
        type Type = #ty;
        type Rest = #tail;
        const OFFSET: usize = #root::offset_of!(#ident #ty_generics, #field_ident);
        const FIELDS: usize = #fields;
      }
    });
    quote!(#struct_ident)
  });

  // Compute header size.
  let header = match repr {
    Repr::Rust => quote!(::core::mem::size_of::<Self>()),
    Repr::C | Repr::Transparant => quote!({
      let size = #root::offset_of!(Self, #tail_ident) + #root::min_size_of::<#tail_ty>();
      if !#root::is_sized::<#tail_ty>() {
        size
      } else {
        let align = #root::align_of::<Self>();
        size.wrapping_add(align).wrapping_sub(1) & !align.wrapping_sub(1)
      }
    }),
  };

  proc_macro::TokenStream::from(quote!(
    const _: () = {
      #(#impl_fields)*

      unsafe impl #impl_generics #root::Data for #ident #ty_generics #where_clause {
        type Item = ();
        type Tail = <#tail_ty as #root::Data>::Tail;
        type Repr = #fields;

        const LAYOUT: #root::Layout = #root::Layout::Struct;
        const HEADER: usize = #header;
      }
    };
  ))
}
