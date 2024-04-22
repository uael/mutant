use proc_macro2::{Ident, Span, TokenStream};
use quote::{format_ident, quote, ToTokens};
use syn::{
  parenthesized, parse2, parse_macro_input, token, Attribute, Data, DataEnum, DataStruct,
  DeriveInput, Error, Fields, ImplGenerics, Index, Path, TypeGenerics, Visibility, WhereClause,
};

macro_rules! bail {
  ($msg:expr $(,)?) => {
    return Error::new(Span::call_site(), &$msg[..]).to_compile_error().into()
  };
  ($msg:expr => $span_to_blame:expr $(,)?) => {
    return Error::new_spanned(&$span_to_blame, $msg).to_compile_error().into()
  };
}

#[rustfmt::skip]
#[derive(Copy, Clone)]
enum Scalar {
  U8, U16, U32, U64, U128, Usize,
  I8, I16, I32, I64, I128, Isize,
}

impl ToTokens for Scalar {
  fn to_tokens(&self, tokens: &mut TokenStream) {
    tokens.extend(match self {
      Scalar::U8 => quote!(u8),
      Scalar::U16 => quote!(u16),
      Scalar::U32 => quote!(u32),
      Scalar::U64 => quote!(u64),
      Scalar::U128 => quote!(u128),
      Scalar::Usize => quote!(usize),
      Scalar::I8 => quote!(i8),
      Scalar::I16 => quote!(i16),
      Scalar::I32 => quote!(i32),
      Scalar::I64 => quote!(i64),
      Scalar::I128 => quote!(i128),
      Scalar::Isize => quote!(isize),
    });
  }
}

#[repr(C, u8)]
#[derive(Copy, Clone)]
enum Repr {
  Rust(#[allow(unused)] bool),
  C(bool),
  Transparant,
  Scalar(Scalar),
}

fn parse_derive_attr(attrs: &[Attribute]) -> (Repr, TokenStream) {
  let mut repr = Repr::Rust(false);
  let mut packed = false;
  let mut root = quote!(::mutant);
  for attr in attrs {
    if attr.path().is_ident("repr") {
      attr
        .parse_nested_meta(|meta| {
          // #[repr(C)]
          if meta.path.is_ident("C") {
            repr = Repr::C(packed);
          }
          // #[repr(transparent)]
          if meta.path.is_ident("transparent") {
            repr = Repr::Transparant;
          }
          // #[repr(Rust)]
          if meta.path.is_ident("Rust") {
            repr = Repr::Rust(packed);
          }
          // #[repr(packed)]
          if meta.path.is_ident("packed") {
            packed = true;
            // `packed` modifier is only allowed for `C` and `Rust` repr.
            repr = match repr {
              Repr::C(_) => Repr::C(true),
              _ => Repr::Rust(true),
            };
          }
          // #[repr(u*)] or #[repr(i*)]
          for scalar in
            ["u8", "u16", "u32", "u64", "u128", "usize", "i8", "i16", "i32", "i64", "i128", "isize"]
          {
            if meta.path.is_ident(scalar) {
              repr = Repr::Scalar(match scalar {
                "u8" => Scalar::U8,
                "u16" => Scalar::U16,
                "u32" => Scalar::U32,
                "u64" => Scalar::U64,
                "u128" => Scalar::U128,
                "usize" => Scalar::Usize,
                "i8" => Scalar::I8,
                "i16" => Scalar::I16,
                "i32" => Scalar::I32,
                "i64" => Scalar::I64,
                "i128" => Scalar::I128,
                "isize" => Scalar::Isize,
                _ => unreachable!(),
              });
            }
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
            root = if meta.input.peek(token::Paren) {
              let content;
              parenthesized!(content in meta.input);
              let path: Path = content.parse()?;
              quote!(#path)
            } else {
              quote!(crate)
            };
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

  // Derive `Data` for input.
  let args = (repr, root, input.vis, input.ident, input.generics.split_for_impl());
  proc_macro::TokenStream::from(match input.data {
    Data::Enum(data) => derive_for_enum(args, data),
    Data::Struct(data) => derive_for_struct(args, data),
    _ => bail!("#[derive(Data)] is yet only implemented for struct"),
  })
}

#[allow(clippy::too_many_arguments)]
fn derive_for_enum(
  args: (Repr, TokenStream, Visibility, Ident, (ImplGenerics, TypeGenerics, Option<&WhereClause>)),
  data: DataEnum,
) -> proc_macro2::TokenStream {
  // Unpack arguments.
  let (repr, root, _, ident, (impl_generics, ty_generics, where_clause)) = args;
  // Compute repr scalar type.
  let scalar = match repr {
    Repr::Scalar(scalar) => scalar,
    _ => bail!("#[derive(Data)] on enums requires #[repr(u*|i*)]"),
  };
  // Tokens that prefix `Data` implementation.
  let mut allowed_values = vec![];
  for variant in &data.variants {
    let variant_ident = &variant.ident;
    match variant.fields {
      Fields::Unit => {}
      _ => bail!(format!("variant `{}` is not allowed to have fields", variant_ident)),
    };
    let discriminant = match variant.discriminant.as_ref() {
      Some((_, expr)) => expr,
      None => bail!(format!("variant `{}` must have be assigned to a discriminant", variant_ident)),
    };
    allowed_values.push(discriminant);
  }
  quote! {
    unsafe impl #impl_generics #root::Integer for #ident #ty_generics #where_clause {
      type Bits = #scalar;
      const INT_LAYOUT: &'static #root::IntegerLayout = #root::layout!(#(#allowed_values),*);
    }
  }
}

#[allow(clippy::too_many_arguments)]
fn derive_for_struct(
  args: (Repr, TokenStream, Visibility, Ident, (ImplGenerics, TypeGenerics, Option<&WhereClause>)),
  data: DataStruct,
) -> proc_macro2::TokenStream {
  // Unpack arguments.
  let (repr, root, vis, ident, (impl_generics, ty_generics, where_clause)) = args;
  // Compute fields from input.
  let input_fields = match &data.fields {
    Fields::Named(fields) if !fields.named.is_empty() => &fields.named,
    Fields::Unnamed(fields) if !fields.unnamed.is_empty() => &fields.unnamed,
    // Empty struct, impl as `Layout::Void`.
    _ => {
      return quote! {
        unsafe impl #impl_generics #root::Data for #ident #ty_generics #where_clause {
          type Item = ();
          type Head = ();
          type Tail = ();
          type Repr = ();

          const LAYOUT: #root::Layout = <() as #root::Data>::LAYOUT;
        }
      }
    }
  };

  // Compute head.
  let tail = match input_fields.last() {
    Some(field) => field,
    _ => unreachable!(),
  };
  let tail_ty = &tail.ty;

  // Compute `where` clause.
  let predicates = match repr {
    Repr::Scalar(_) => unreachable!(),
    Repr::Rust(_) => input_fields
      .iter()
      .map(|field| &field.ty)
      .map(|ty| quote!(#ty: #root::Data + Copy))
      .chain([quote!(Self: Sized)])
      .collect::<Vec<_>>(),
    Repr::C(_) | Repr::Transparant => input_fields
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

  // Tokens that prefix `Data` implementation.
  let mut prefix = vec![];

  // Compute `Repr` fields.
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
    prefix.push(quote! {
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

  // Compute head type.
  let head = match repr {
    Repr::Scalar(_) => unreachable!(),
    Repr::Rust(_) => quote!(Self),
    Repr::Transparant => quote!(<#tail_ty as #root::Data>::Head),
    Repr::C(_) if input_fields.len() == 1 => quote!(<#tail_ty as #root::Data>::Head),
    Repr::C(packed) => {
      let ident = format_ident!("__{}Head", ident.to_string());
      let map_tail_type = |new_type| {
        let mut field = tail.clone();
        field.ty = parse2(new_type).unwrap();
        field
      };
      let fields = input_fields
        .iter()
        .take(input_fields.len() - 1)
        .cloned()
        .chain([
          map_tail_type(quote!(<#tail_ty as #root::Data>::Head)),
          map_tail_type(quote!([<#tail_ty as #root::Data>::Tail; 0])),
        ])
        .map(|mut field| {
          field.ident = None;
          field
        })
        .collect::<Vec<_>>();
      let repr = fields.iter().enumerate().rfold(quote!(()), |tail, (i, head)| {
        let ty = &head.ty;
        let field_ident = {
          let ident = Index { index: i as u32, span: Span::call_site() };
          quote!(#ident)
        };
        let struct_ident = format_ident!("__{}{}", ident.to_string(), i);
        let fields = fields.len() - i;
        prefix.push(quote! {
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
      let field_types = fields.iter().map(|field| &field.ty);
      let repr_attr = if packed { quote!(#[repr(packed)]) } else { quote!(#[repr(C, packed)]) };
      prefix.push(quote! {
        #[allow(non_camel_case_types)]
        #repr_attr
        #vis struct #ident #impl_generics (#(#field_types),*) #where_clause;

        impl #impl_generics ::core::marker::Copy for #ident #ty_generics #where_clause {}

        impl #impl_generics ::core::clone::Clone for #ident #ty_generics #where_clause {
          #[inline(always)]
          fn clone(&self) -> Self {
            *self
          }
        }

        unsafe impl #impl_generics #root::Data for #ident #ty_generics #where_clause {
          type Item = ();
          type Head = Self;
          type Tail = ();
          type Repr = #repr;

          const LAYOUT: #root::Layout = #root::Layout::Struct;
        }
      });
      quote!(#ident #ty_generics)
    }
  };

  quote!(
    const _: () = {
      #(#prefix)*

      unsafe impl #impl_generics #root::Data for #ident #ty_generics #where_clause {
        type Item = ();
        type Head = #head;
        type Tail = <#tail_ty as #root::Data>::Tail;
        type Repr = #fields;

        const LAYOUT: #root::Layout = #root::Layout::Struct;
      }
    };
  )
}
