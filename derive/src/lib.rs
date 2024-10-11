use proc_macro2::{Ident, Span, TokenStream};
use quote::{format_ident, quote, ToTokens};
use syn::punctuated::Punctuated;
use syn::token::Comma;
use syn::{
  parenthesized, parse_macro_input, token, Attribute, Data, DataEnum, DataStruct,
  DeriveInput, Error, Field, Fields, ImplGenerics, Index, Path, TypeGenerics, Visibility,
  WhereClause,
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
  Rust(bool),
  C(bool),
  Transparant,
  Scalar(Scalar),
}

impl Repr {
  pub const fn is_packed(&self) -> bool {
    match self {
      Repr::Rust(packed) | Repr::C(packed) => *packed,
      _ => false,
    }
  }
}

#[proc_macro_derive(Type, attributes(mutant))]
pub fn derive_type(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
  let input = parse_macro_input!(input as DeriveInput);
  let (repr, root) = parse_derive_attr(&input.attrs);

  // Derive `Data` for input.
  let args = (repr, root, input.vis, input.ident, input.generics.split_for_impl());
  proc_macro::TokenStream::from(match input.data {
    Data::Enum(data) => derive_type_for_enum(args, data),
    Data::Struct(data) => derive_type_for_struct(args, data),
    _ => bail!("#[derive(Type)] is yet only implemented for struct and enum"),
  })
}

#[proc_macro_derive(DebugPacked, attributes(mutant))]
pub fn derive_debug_packed(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
  let input = parse_macro_input!(input as DeriveInput);
  let (repr, root) = parse_derive_attr(&input.attrs);

  // Derive `DataDebug` for input.
  let args = (repr, root, input.vis, input.ident, input.generics.split_for_impl());
  proc_macro::TokenStream::from(match input.data {
    Data::Struct(data) => derive_debug_packed_for_struct(args, data),
    _ => bail!("#[derive(DebugPacked)] is yet only implemented for struct"),
  })
}

#[proc_macro_derive(PartialEqPacked, attributes(mutant))]
pub fn derive_partial_eq_packed(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
  let input = parse_macro_input!(input as DeriveInput);
  let (repr, root) = parse_derive_attr(&input.attrs);

  // Derive `DataDebug` for input.
  let args = (repr, root, input.vis, input.ident, input.generics.split_for_impl());
  proc_macro::TokenStream::from(match input.data {
    Data::Struct(data) => derive_partial_eq_packed_for_struct(args, data),
    _ => bail!("#[derive(PartialEqPacked)] is yet only implemented for struct"),
  })
}

fn derive_type_for_enum(
  args: (Repr, TokenStream, Visibility, Ident, (ImplGenerics, TypeGenerics, Option<&WhereClause>)),
  data: DataEnum,
) -> proc_macro2::TokenStream {
  // Unpack arguments.
  let (repr, root, _, ident, (impl_generics, ty_generics, where_clause)) = args;
  // Compute repr scalar type.
  let scalar = match repr {
    Repr::Scalar(scalar) => scalar,
    _ => bail!("#[derive(Type)] on enums requires #[repr(u*|i*)]"),
  };
  // Tokens that prefix `Type` implementation.
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
      type Repr = #scalar;

      const SPEC: #root::IntegerSpec = #root::IntegerSpec::allow(&[#(#allowed_values),*]);
    }
  }
}

fn derive_type_for_struct(
  args: (Repr, TokenStream, Visibility, Ident, (ImplGenerics, TypeGenerics, Option<&WhereClause>)),
  data: DataStruct,
) -> proc_macro2::TokenStream {
  // Unpack arguments.
  let (repr, root, vis, ident, (impl_generics, ty_generics, where_clause)) = args;
  // Compute fields from input.
  let bound = Some(quote!(+ #root::Type+ #root::ptr::Pointee));
  let (input_fields, where_clause) =
    match preprocess_data_struct(data, repr, &root, where_clause, bound) {
      Some(fields) => fields,
      // Empty struct, impl as `()`.
      None => {
        return quote! {
          unsafe impl #impl_generics #root::ptr::Pointee for #ident #ty_generics #where_clause {
            type Metadata = ();
          }

          unsafe impl #impl_generics #root::Type for #ident #ty_generics #where_clause {
            type Sizing = #root::sizing::Sized;
            type Layout = ();
          }
        }
      }
    };

  // Compute tail.
  let tail = match input_fields.last() {
    Some(field) => field,
    _ => unreachable!(),
  };
  let tail_ty = &tail.ty;
  let tail_ident = quote_field_ident(tail, input_fields.len() - 1);

  // Tokens that prefix `Type` implementation.
  let mut prefix = vec![];

  // Compute `Pointee` assocative types.
  let metadata_ty = quote!(<#tail_ty as #root::ptr::Pointee>::Metadata);

  // Compute `Type` assocative types.
  let sizing_ty = quote!(<#tail_ty as #root::Type>::Sizing);
  let layout_ty = input_fields
    .iter()
    .enumerate()
    .map(|(i, field)| {
      let ty = &field.ty;
      let field_ident = quote_field_ident(field, i);
      let struct_ident = format_ident!("__{}{}Offset", ident.to_string(), i);
      prefix.push(quote! {
        #vis struct #struct_ident #impl_generics(::core::marker::PhantomData<#ident #ty_generics>)
          #where_clause;

        impl #impl_generics #root::Literal<usize> for #struct_ident #ty_generics #where_clause {
          const LITERAL: usize = #root::offset_of!(#ident #ty_generics, #field_ident);
        }
      });
      quote!((#struct_ident #ty_generics, <#ty as #root::Type>::Layout))
    })
    .collect::<Vec<_>>();

  quote!(
    const _: () = {
      #(#prefix)*

      unsafe impl #impl_generics #root::ptr::Pointee
        for #ident #ty_generics #where_clause
      {
        type Metadata = #metadata_ty;
      }

      impl #impl_generics #root::Type
        for #ident #ty_generics #where_clause Self: #root::MinimallySized<#sizing_ty>
      {
        type Sizing = #sizing_ty;
        type Layout = (#(#layout_ty,)*);
      }

      impl #impl_generics #root::MinimallySized<#root::sizing::Unsized>
        for #ident #ty_generics #where_clause
      {
        const MINSIZE: usize = if #root::is_unsized::<#tail_ty>() {
          #root::offset_of!(Self, #tail_ident) + #root::min_size_of::<#tail_ty>()
        } else {
          panic!("use of `MINSIZE` with an invalid `Unsized` sizing")
        };
      }
    };
  )
}

fn derive_debug_packed_for_struct(
  args: (Repr, TokenStream, Visibility, Ident, (ImplGenerics, TypeGenerics, Option<&WhereClause>)),
  data: DataStruct,
) -> proc_macro2::TokenStream {
  // Unpack arguments.
  let (repr, root, _, ident, (impl_generics, ty_generics, where_clause)) = args;
  // Check for `packed`.
  if !repr.is_packed() {
    bail!(
      "#[derive(DebugPacked)] is only allowed for #[repr(packed)] or #[repr(C, packed)] structs"
    );
  }
  // Compute fields from input.
  let (input_fields, where_clause) = match preprocess_data_struct(
    data,
    repr,
    &root,
    where_clause,
    Some(quote!(+ ::core::fmt::Debug)),
  ) {
    Some(fields) => fields,
    None => {
      return quote! {
        unsafe impl #impl_generics ::core::fmt::Debug for #ident #ty_generics #where_clause {
          #[inline]
          fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
            f.debug_struct(::core::any::type_name::<Self>()).finish()
          }
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
  let where_clause = if tail_ty.to_token_stream().to_string().contains("ManuallyDrop") {
    let tail_predicate = quote!(<#tail_ty as ::core::ops::Deref>::Target: ::core::fmt::Debug);
    quote!(#where_clause #tail_predicate)
  } else {
    where_clause
  };

  let statements = input_fields
    .iter()
    .take(input_fields.len() - 1)
    .enumerate()
    .map(|(i, field)| {
      let ident = quote_field_ident(field, i);
      quote!(d.field(stringify!(#ident), &{ self.#ident });)
    })
    .chain([{
      let ident = quote_field_ident(tail, input_fields.len() - 1);
      let deref = if tail_ty.to_token_stream().to_string().contains("ManuallyDrop") {
        quote!(.deref())
      } else {
        quote!()
      };
      quote!(d.field(stringify!(#ident), &#root::unaligned_ref!(self.#ident in tail) #deref);)
    }]);

  quote! {
    impl #impl_generics ::core::fmt::Debug for #ident #ty_generics #where_clause {
      #[inline]
      fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
        use ::core::ops::Deref;
        let tail;
        let mut d = f.debug_struct(::core::any::type_name::<Self>());
        #(#statements)*
        d.finish()
      }
    }
  }
}

fn derive_partial_eq_packed_for_struct(
  args: (Repr, TokenStream, Visibility, Ident, (ImplGenerics, TypeGenerics, Option<&WhereClause>)),
  data: DataStruct,
) -> proc_macro2::TokenStream {
  // Unpack arguments.
  let (repr, root, _, ident, (impl_generics, ty_generics, where_clause)) = args;
  // Check for `packed`.
  if !repr.is_packed() {
    bail!(
      "#[derive(PartialEqPacked)] is only allowed for #[repr(packed)] or #[repr(C, packed)] \
       structs"
    );
  }
  // Compute fields from input.
  let (input_fields, where_clause) = match preprocess_data_struct(
    data,
    repr,
    &root,
    where_clause,
    Some(quote!(+ ::core::cmp::PartialEq)),
  ) {
    Some(fields) => fields,
    None => {
      return quote! {
        unsafe impl #impl_generics ::core::cmp::PartialEq for #ident #ty_generics #where_clause {
          #[inline]
          fn eq(&self, other: &Self) -> bool {
            true
          }
        }
      }
    }
  };

  // Compute head.
  let tail = match input_fields.last() {
    Some(field) => field,
    _ => unreachable!(),
  };

  let statements = input_fields
    .iter()
    .take(input_fields.len() - 1)
    .enumerate()
    .map(|(i, field)| {
      let ident = quote_field_ident(field, i);
      quote!(if !{ self.#ident }.eq(&{ other.#ident }) { return false; })
    })
    .chain([{
      let ident = quote_field_ident(tail, input_fields.len() - 1);
      quote!(#root::unaligned_ref!(self.#ident in a).eq(#root::unaligned_ref!(other.#ident in b)))
    }]);

  quote! {
    impl #impl_generics ::core::cmp::PartialEq for #ident #ty_generics #where_clause {
      #[inline]
      fn eq(&self, other: &Self) -> bool {
        let (a, b);
        #(#statements)*
      }
    }
  }
}

/// Quote field ident.
fn quote_field_ident(field: &Field, i: usize) -> TokenStream {
  match field.ident.as_ref() {
    Some(ident) => quote!(#ident),
    _ => {
      let ident = Index { index: i as u32, span: Span::call_site() };
      quote!(#ident)
    }
  }
}

/// Parse `#[repr]` and `#[data]` attributes.
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
    if attr.path().is_ident("mutant") {
      attr
        .parse_nested_meta(|meta| {
          // #[mutant(crate(<Path>))]
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

          Err(meta.error("unrecognized mutant attribute"))
        })
        .unwrap();
    }
  }
  (repr, root)
}

/// Preprocess data struct.
fn preprocess_data_struct(
  data: DataStruct, repr: Repr, root: &TokenStream, where_clause: Option<&WhereClause>,
  bound: Option<TokenStream>,
) -> Option<(Punctuated<Field, Comma>, TokenStream)> {
  // Compute fields from input.
  let input_fields = match data.fields {
    Fields::Named(fields) if !fields.named.is_empty() => fields.named,
    Fields::Unnamed(fields) if !fields.unnamed.is_empty() => fields.unnamed,
    _ => return None,
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
      .map(|ty| quote!(#ty: ::core::marker::Copy #bound))
      .chain([quote!(Self: ::core::marker::Sized)])
      .collect::<Vec<_>>(),
    Repr::C(_) | Repr::Transparant => input_fields
      .iter()
      .take(input_fields.len() - 1)
      .map(|field| &field.ty)
      .map(|ty| quote!(#ty: ::core::marker::Copy #bound))
      .chain([quote!(#tail_ty: #root::Type #bound)])
      .collect::<Vec<_>>(),
  };
  let where_clause = match where_clause {
    None => quote!(where #(#predicates,)*),
    Some(where_clause) => {
      let existing_predicates = where_clause.predicates.iter();
      quote!(where #(#existing_predicates,)* #(#predicates,)*)
    }
  };

  Some((input_fields, where_clause))
}
