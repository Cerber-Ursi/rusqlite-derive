use attribute_derive::FromAttr;

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
    let syn::Data::Struct(data) = input.data else {
        return Err(syn::Error::new_spanned(
            input.ident,
            "only structs are supported for now",
        ));
    };

    let name = input.ident;

    let table_attr = RusqliteTable::from_attributes(&input.attrs)?;
    let table_name = table_attr.from.unwrap_or_else(|| name.to_string());

    let mut columns = vec![];
    let mut fields = vec![];

    for (index, field) in data.fields.into_iter().enumerate() {
        let name = field.ident.as_ref().ok_or_else(|| {
            syn::Error::new_spanned(
                &field,
                "only structs with named fields are supported for now",
            )
        })?;

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
