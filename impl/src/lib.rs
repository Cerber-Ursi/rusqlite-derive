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
/// its select expression. Every field of a tuple struct must specify `select`
/// because it has no field name to use as a default. These values are inserted
/// as SQL, allowing qualified columns, expressions, aliases, and joins.
///
/// Selected values are decoded by field declaration order with
/// `rusqlite::Row::get`.
///
/// # Security
///
/// `fetch_with_filter` inserts its filter argument into the statement verbatim.
/// It provides no escaping or parameter binding, so the argument must never
/// contain untrusted input.
///
/// # Limitations
///
/// The macro currently supports non-generic structs with one or more fields.
/// Unit structs are not supported.
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
    select: Option<String>,
}

fn fetch(input: syn::DeriveInput) -> syn::Result<proc_macro2::TokenStream> {
    if !input.generics.params.is_empty() {
        return Err(syn::Error::new_spanned(
            input.generics,
            "generic structs are not supported for now",
        ));
    }

    let syn::Data::Struct(data) = input.data else {
        return Err(syn::Error::new_spanned(
            input.ident,
            "only structs are supported for now",
        ));
    };

    let name = input.ident;

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

            for (index, field) in fields.named.into_iter().enumerate() {
                let field_name = field
                    .ident
                    .as_ref()
                    .expect("fields were checked to be named");
                let column_attr = RusqliteColumn::from_attributes(&field.attrs)?;
                let column = column_attr.select.unwrap_or_else(|| field_name.to_string());

                columns.push(column);
                values.push(quote::quote! { #field_name: row.get(#index)?, });
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

            for (index, field) in fields.unnamed.into_iter().enumerate() {
                let column_attr = RusqliteColumn::from_attributes(&field.attrs)?;
                let column = column_attr.select.ok_or_else(|| {
                    syn::Error::new_spanned(
                        &field,
                        "tuple struct fields require #[rusqlite(select = \"...\")]",
                    )
                })?;

                columns.push(column);
                values.push(quote::quote! { row.get(#index)?, });
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

    let query_simple = format!("SELECT {} FROM {};", columns.join(", "), table_name);
    let query_with_where = format!(
        "SELECT {} FROM {} WHERE {{}};",
        columns.join(", "),
        table_name
    );

    let res = quote::quote! {
        impl ::rusqlite_derive::RusqliteFetch for #name {
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
