//! Implementation of the `RusqliteFetch` derive macro.
//!
//! Applications should depend on and import the `rusqlite-derive` wrapper crate,
//! which exposes both the derive and the trait it implements.

use attribute_derive::FromAttr;
use syn::spanned::Spanned;

/// Derives SQLite read helpers for a struct.
///
/// `#[rusqlite(table = "...")]` sets the default read source.
/// `#[rusqlite(from = "...")]` overrides that source with a complete `FROM`
/// fragment. Named fields default to their Rust names;
/// `select` sets a read expression, `column` sets a storage column and default
/// read expression, and `read_default` omits a field from reads and initializes
/// it with `Default::default()`.
#[proc_macro_derive(RusqliteFetch, attributes(rusqlite))]
pub fn derive_fetch(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let def = syn::parse_macro_input!(input as syn::DeriveInput);

    match fetch(def) {
        Ok(ts) => ts,
        Err(e) => e.into_compile_error(),
    }
    .into()
}

#[derive(FromAttr)]
#[attribute(ident = rusqlite)]
struct RusqliteTable {
    from: Option<String>,
    table: Option<String>,
}

#[derive(FromAttr)]
#[attribute(ident = rusqlite)]
struct RusqliteField {
    #[attribute(conflicts = [read_default])]
    select: Option<String>,
    column: Option<String>,
    #[attribute(conflicts = [select])]
    read_default: bool,
}

fn fetch(input: syn::DeriveInput) -> syn::Result<proc_macro2::TokenStream> {
    let syn::Data::Struct(data) = input.data else {
        return Err(syn::Error::new_spanned(
            input.ident,
            "only structs are supported for now",
        ));
    };

    let name = input.ident;
    let mut generics = input.generics;

    let table_attr = RusqliteTable::from_attributes(&input.attrs)?;
    let source = table_attr
        .from
        .or(table_attr.table)
        .unwrap_or_else(|| name.to_string());

    let (columns, row_value) = match data.fields {
        syn::Fields::Named(fields) => {
            let mut columns = vec![];
            let mut values = vec![];

            for field in fields.named {
                let field_name = field
                    .ident
                    .as_ref()
                    .expect("fields were checked to be named");
                let attr = RusqliteField::from_attributes(&field.attrs)?;
                let ty = &field.ty;

                if attr.read_default {
                    add_field_bound(&mut generics, ty, quote::quote!(::core::default::Default));
                    let value = quote::quote_spanned! { ty.span() =>
                        ::core::default::Default::default()
                    };
                    values.push(quote::quote! { #field_name: #value, });
                } else {
                    let index = columns.len();
                    let column = attr
                        .select
                        .or(attr.column)
                        .unwrap_or_else(|| field_name.to_string());
                    columns.push(column);
                    add_field_bound(&mut generics, ty, quote::quote!(::rusqlite::types::FromSql));
                    let value = quote::quote_spanned! { ty.span() =>
                        row.get::<_, #ty>(#index)?
                    };
                    values.push(quote::quote! { #field_name: #value, });
                }
            }

            (columns, quote::quote! { #name { #(#values)* } })
        }
        syn::Fields::Unnamed(fields) => {
            let mut columns = vec![];
            let mut values = vec![];

            for field in fields.unnamed {
                let attr = RusqliteField::from_attributes(&field.attrs)?;
                let ty = &field.ty;

                if attr.read_default {
                    add_field_bound(&mut generics, ty, quote::quote!(::core::default::Default));
                    values.push(quote::quote_spanned! { ty.span() =>
                        ::core::default::Default::default(),
                    });
                } else {
                    let column = attr.select.or(attr.column).ok_or_else(|| {
                        syn::Error::new_spanned(
                            &field,
                            "tuple struct fields require #[rusqlite(select = \"...\")], #[rusqlite(column = \"...\")], or #[rusqlite(read_default)]",
                        )
                    })?;
                    let index = columns.len();
                    columns.push(column);
                    add_field_bound(&mut generics, ty, quote::quote!(::rusqlite::types::FromSql));
                    values.push(quote::quote_spanned! { ty.span() =>
                        row.get::<_, #ty>(#index)?,
                    });
                }
            }

            (columns, quote::quote! { #name(#(#values)*) })
        }
        syn::Fields::Unit => (vec![], quote::quote! { #name }),
    };

    // SQLite still needs a result expression when every field is defaulted.
    // Selecting a constant preserves one constructed value per source row.
    let select = if columns.is_empty() {
        "1".to_owned()
    } else {
        columns.join(", ")
    };
    let query_simple = format!("SELECT {select} FROM {source};");
    let query_with_where = format!("SELECT {select} FROM {source} WHERE {{}};");
    let (impl_generics, type_generics, where_clause) = generics.split_for_impl();

    Ok(quote::quote! {
        impl #impl_generics ::rusqlite_derive::RusqliteFetch for #name #type_generics #where_clause {
            fn fetch(conn: &::rusqlite::Connection) -> ::rusqlite::Result<Vec<Self>> {
                conn
                    .prepare(#query_simple)?
                    .query_map([], |row| {
                        Ok(#row_value)
                    })?
                    .collect()
            }

            fn fetch_with_filter<P: ::rusqlite::Params>(
                conn: &::rusqlite::Connection,
                filter: &str,
                params: P,
            ) -> ::rusqlite::Result<Vec<Self>> {
                conn
                    .prepare(&format!(#query_with_where, filter))?
                    .query_map(params, |row| {
                        Ok(#row_value)
                    })?
                    .collect()
            }
        }
    })
}

fn add_field_bound(
    generics: &mut syn::Generics,
    ty: &syn::Type,
    trait_path: proc_macro2::TokenStream,
) {
    use quote::ToTokens;

    // A concrete field is checked at the generated operation whose span is the
    // field type. Only generic-dependent fields need an impl bound.
    let parameter_names: Vec<_> = generics
        .params
        .iter()
        .map(|parameter| match parameter {
            syn::GenericParam::Type(parameter) => parameter.ident.to_string(),
            syn::GenericParam::Lifetime(parameter) => parameter.lifetime.ident.to_string(),
            syn::GenericParam::Const(parameter) => parameter.ident.to_string(),
        })
        .collect();
    if !contains_parameter(ty.to_token_stream(), &parameter_names) {
        return;
    }

    let predicate = syn::parse_quote_spanned! {ty.span()=> #ty: #trait_path};
    generics.make_where_clause().predicates.push(predicate);
}

fn contains_parameter(tokens: proc_macro2::TokenStream, parameters: &[String]) -> bool {
    tokens.into_iter().any(|token| match token {
        proc_macro2::TokenTree::Ident(ident) => parameters.contains(&ident.to_string()),
        proc_macro2::TokenTree::Group(group) => contains_parameter(group.stream(), parameters),
        _ => false,
    })
}
