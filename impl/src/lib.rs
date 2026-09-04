//! Implementation of the `RusqliteFetch` derive macro.
//!
//! Applications should depend on and import the `rusqlite-derive` wrapper crate,
//! which exposes both the derive and the trait it implements.

use attribute_derive::FromAttr;

/// Derives read helpers for a struct with named fields.
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
/// its select expression. These values are inserted as SQL, allowing qualified
/// columns, expressions, aliases, and joins.
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
/// The macro currently supports non-generic structs with one or more named
/// fields only.
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

    let named_fields = match data.fields {
        syn::Fields::Named(fields) => fields,
        other => {
            return Err(syn::Error::new_spanned(
                other,
                "only structs with named fields are supported for now",
            ));
        }
    };

    if named_fields.named.is_empty() {
        return Err(syn::Error::new_spanned(
            named_fields,
            "structs must have at least one field",
        ));
    }

    let name = input.ident;

    let table_attr = RusqliteTable::from_attributes(&input.attrs)?;
    let table_name = table_attr.from.unwrap_or_else(|| name.to_string());

    let mut columns = vec![];
    let mut fields = vec![];

    for (index, field) in named_fields.named.into_iter().enumerate() {
        let name = field
            .ident
            .as_ref()
            .expect("fields were checked to be named");

        let column_attr = RusqliteColumn::from_attributes(field.attrs)?;
        let column = column_attr.select.unwrap_or_else(|| name.to_string());

        columns.push(column);
        fields.push(quote::quote! { #name: row.get(#index)?, });
    }

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
                        Ok(#name {
                            #(#fields)*
                        })
                    })?
                    .collect()
            }
            fn fetch_with_filter(conn: &rusqlite::Connection, filter: &str) -> rusqlite::Result<Vec<Self>> {
                conn
                    .prepare(&format!(#query_with_where, filter))?
                    .query_map([], |row| {
                        Ok(#name {
                            #(#fields)*
                        })
                    })?
                    .collect()
            }
        }
    };

    Ok(res)
}
