//! Implementation of the `RusqliteFetch` derive macro.
//!
//! Applications should depend on and import the `rusqlite-derive` wrapper crate,
//! which exposes both the derive and the trait it implements.

use attribute_derive::FromAttr;

/// Derives read helpers for a struct with named or unnamed fields.
///
/// By default, the macro uses the struct name as the SQL `FROM` fragment and
/// each field name as a select expression. The generated implementation executes
/// statements equivalent to:
///
/// ```sql
/// SELECT field_1, field_2 FROM StructName;
/// SELECT field_1, field_2 FROM StructName WHERE <filter>;
/// ```
///
/// Use `#[rusqlite(from = "...")]` on the struct to override the complete
/// `FROM` fragment. Use `#[rusqlite(select = "...")]` on a field to override
/// its select expression, or `#[rusqlite(default)]` to omit the field from the
/// query and initialize it with `Default::default()`. Every non-default field
/// of a tuple struct must specify `select` because it has no field name to use
/// as a default. SQL values are inserted verbatim, allowing qualified columns,
/// expressions, aliases, and joins.
///
/// Selected values are decoded by field declaration order with
/// `rusqlite::Row::get`. Generic structs are supported; the generated impl is
/// constrained by `FromSql` for selected field types and `Default` for defaulted
/// field types.
///
/// # Security
///
/// `fetch_with_filter` inserts its filter argument into the statement verbatim.
/// It provides no escaping or parameter binding, so the argument must never
/// contain untrusted input.
///
/// # Limitations
///
/// The macro supports structs with one or more fields. Unit structs are not
/// supported.
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
}

#[derive(FromAttr)]
#[attribute(ident = rusqlite)]
struct RusqliteColumn {
    #[attribute(conflicts = [default])]
    select: Option<String>,
    #[attribute(conflicts = [select])]
    default: bool,
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
    let table_name = table_attr.from.unwrap_or_else(|| name.to_string());

    let (columns, row_value) = match data.fields {
        syn::Fields::Named(fields) => {
            if fields.named.is_empty() {
                return Err(syn::Error::new_spanned(
                    fields,
                    "structs must have at least one field",
                ));
            }

            let mut columns = vec![];
            let mut values = vec![];

            for field in fields.named {
                let field_name = field
                    .ident
                    .as_ref()
                    .expect("fields were checked to be named");
                let column_attr = RusqliteColumn::from_attributes(&field.attrs)?;
                let ty = &field.ty;

                if column_attr.default {
                    add_field_bound(&mut generics, ty, quote::quote!(::core::default::Default));
                    let value = quote::quote_spanned! { syn::spanned::Spanned::span(ty) =>
                        ::core::default::Default::default()
                    };
                    values.push(quote::quote! { #field_name: #value, });
                } else {
                    let index = columns.len();
                    let column = column_attr.select.unwrap_or_else(|| field_name.to_string());
                    columns.push(column);
                    add_field_bound(&mut generics, ty, quote::quote!(::rusqlite::types::FromSql));
                    let value = quote::quote_spanned! { syn::spanned::Spanned::span(ty) =>
                        row.get::<_, #ty>(#index)?
                    };
                    values.push(quote::quote! { #field_name: #value, });
                }
            }

            (columns, quote::quote! { #name { #(#values)* } })
        }
        syn::Fields::Unnamed(fields) => {
            if fields.unnamed.is_empty() {
                return Err(syn::Error::new_spanned(
                    fields,
                    "structs must have at least one field",
                ));
            }

            let mut columns = vec![];
            let mut values = vec![];

            for field in fields.unnamed {
                let column_attr = RusqliteColumn::from_attributes(&field.attrs)?;
                let ty = &field.ty;

                if column_attr.default {
                    add_field_bound(&mut generics, ty, quote::quote!(::core::default::Default));
                    values.push(quote::quote_spanned! { syn::spanned::Spanned::span(ty) =>
                        ::core::default::Default::default(),
                    });
                } else {
                    let column = column_attr.select.ok_or_else(|| {
                        syn::Error::new_spanned(
                            &field,
                            "tuple struct fields require #[rusqlite(select = \"...\")] or #[rusqlite(default)]",
                        )
                    })?;
                    let index = columns.len();
                    columns.push(column);
                    add_field_bound(&mut generics, ty, quote::quote!(::rusqlite::types::FromSql));
                    values.push(quote::quote_spanned! { syn::spanned::Spanned::span(ty) =>
                        row.get::<_, #ty>(#index)?,
                    });
                }
            }

            (columns, quote::quote! { #name(#(#values)*) })
        }
        syn::Fields::Unit => {
            return Err(syn::Error::new_spanned(
                name,
                "unit structs are not supported for now",
            ));
        }
    };

    // SQLite still needs a result expression when every field is defaulted.
    // Selecting a constant preserves one constructed value per source row.
    let select = if columns.is_empty() {
        "1".to_owned()
    } else {
        columns.join(", ")
    };
    let query_simple = format!("SELECT {select} FROM {table_name};");
    let query_with_where = format!("SELECT {select} FROM {table_name} WHERE {{}};");
    let (impl_generics, type_generics, where_clause) = generics.split_for_impl();

    let res = quote::quote! {
        impl #impl_generics ::rusqlite_derive::RusqliteFetch for #name #type_generics #where_clause {
            fn fetch(conn: &rusqlite::Connection) -> rusqlite::Result<Vec<Self>> {
                conn
                    .prepare(#query_simple)?
                    .query_map([], |row| {
                        Ok(#row_value)
                    })?
                    .collect()
            }
            fn fetch_with_filter(conn: &rusqlite::Connection, filter: &str) -> rusqlite::Result<Vec<Self>> {
                conn
                    .prepare(&format!(#query_with_where, filter))?
                    .query_map([], |row| {
                        Ok(#row_value)
                    })?
                    .collect()
            }
        }
    };

    Ok(res)
}

fn add_field_bound(
    generics: &mut syn::Generics,
    ty: &syn::Type,
    trait_path: proc_macro2::TokenStream,
) {
    use quote::ToTokens;
    use syn::spanned::Spanned;

    // A concrete field is checked at the generated `row.get`/`default` call,
    // whose span is the field type. Only generic-dependent fields need an impl
    // bound; adding bounds for concrete fields makes rustc blame the derive
    // invocation instead of the offending type.
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
