use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote, ToTokens};
use syn::{parse_macro_input, Data, DeriveInput, Error, Fields, Index};

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

fn crate_name(input: &DeriveInput) -> TokenStream {
  const ATTR_NAME: &'static str = "crate";

  let mut crate_name = quote!(::mutant);
  for attr in &input.attrs {
    if !attr.path().is_ident("mutant") {
      continue;
    }

    attr
      .parse_nested_meta(|meta| {
        if meta.path.is_ident(ATTR_NAME) {
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
            bail!("Expected mutant `crate` attribute to be a string: `crate = \"...\"`",)
          }
        }
        Ok(())
      })
      .unwrap();
  }

  return crate_name;
}

#[proc_macro_derive(Mutant, attributes(mutant))]
pub fn derive_mutant(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
  let input = parse_macro_input!(input as DeriveInput);
  let mutant = crate_name(&input);

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

          const HEAD: #mutant::Layout<Self::Head> = <()>::HEAD;
          const TAIL: #mutant::Layout<Self::Tail> = <()>::TAIL;
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
      [<#last_ty as #mutant::Mutant>::Tail; 0]
    ) #where_clause;

    impl #impl_generics Copy for #head #ty_generics #where_clause { }

    impl #impl_generics Clone for #head #ty_generics #where_clause {
      fn clone(&self) -> Self {
        *self
      }
    }

    unsafe impl #impl_generics #mutant::Mutant for #this #ty_generics #where_clause {
      type Head = #head #ty_generics;
      type Tail = <#last_ty as #mutant::Mutant>::Tail;

      const HEAD: #mutant::Layout<Self::Head> = unsafe {
        let mut layout: Self::Head = ::core::mem::zeroed();
        #(#head_inits;)*
        #mutant::Layout::new(layout)
      };
      const TAIL: #mutant::Layout<Self::Tail> = <#last_ty as #mutant::Mutant>::TAIL;
    }
  )
  .into()
}
