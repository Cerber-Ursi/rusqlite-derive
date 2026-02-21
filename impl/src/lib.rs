#[proc_macro_derive(RusqliteFetch, attributes(rusqlite))]
pub fn derive_fetch(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let def = syn::parse_macro_input!(input as syn::DeriveInput);

    match fetch(def) {
        Ok(ts) => ts,
        Err(e) => e.into_compile_error(),
    }
    .into()
}

fn fetch(input: syn::DeriveInput) -> syn::Result<proc_macro2::TokenStream> {
    let syn::Data::Struct(data) = input.data else {
        return Err(syn::Error::new_spanned(
            input.ident,
            "only structs are supported for now",
        ));
    };

    let name = input.ident;

    let mut table_name = name.to_string();
    for attr in input.attrs {
        if attr.path().is_ident("rusqlite") {
            let nested = attr.parse_args_with(
                syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated,
            )?;

            for meta in nested {
                let meta = meta.require_name_value()?;

                if meta.path.is_ident("table") {
                    let syn::Expr::Lit(lit) = &meta.value else {
                        return Err(syn::Error::new_spanned(
                            meta,
                            "expected 'table = \"table-name\"",
                        ));
                    };

                    let syn::Lit::Str(lit) = &lit.lit else {
                        return Err(syn::Error::new_spanned(
                            meta,
                            "expected 'table = \"table-name\"",
                        ));
                    };

                    table_name = lit.value();
                }
            }
        }
    }

    let mut columns = vec![];
    let mut fields = vec![];

    for (index, field) in data.fields.into_iter().enumerate() {
        let name = field.ident.expect("TODO - support tuple structs");

        let mut column = name.to_string();
        for attr in field.attrs {
            if attr.path().is_ident("rusqlite") {
                let nested = attr.parse_args_with(
                    syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated,
                )?;

                for meta in nested {
                    let meta = meta.require_name_value()?;

                    if meta.path.is_ident("table") {
                        let syn::Expr::Lit(lit) = &meta.value else {
                            return Err(syn::Error::new_spanned(
                                meta,
                                "expected 'table = \"table-name\"",
                            ));
                        };

                        let syn::Lit::Str(lit) = &lit.lit else {
                            return Err(syn::Error::new_spanned(
                                meta,
                                "expected 'table = \"table-name\"",
                            ));
                        };

                        column = lit.value();
                    }
                }
            }
        }

        columns.push(column);
        fields.push(quote::quote! { #name: row.get(#index)?, });
    }

    let query_simple = format!("SELECT {} FROM {};", columns.join(", "), table_name);
    let query_with_where = format!("SELECT {} FROM {} WHERE {{}};", columns.join(", "), table_name);

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
