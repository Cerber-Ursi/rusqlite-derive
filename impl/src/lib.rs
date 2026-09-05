//! Implementation of the `RusqliteFetch` and `RusqliteWrite` derive macros.
//!
//! Applications should depend on and import the `rusqlite-derive` wrapper crate,
//! which exposes the derives and the traits they implement.

use attribute_derive::FromAttr;
use syn::{ext::IdentExt, spanned::Spanned};

/// Derives SQLite read helpers for a struct.
///
/// `#[rusqlite(table = "...")]` sets the default read source and can also be
/// used by `RusqliteWrite`. `#[rusqlite(from = "...")]` overrides that source
/// with a complete `FROM` fragment. Named fields default to their Rust names;
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

/// Derives complete-record SQLite write helpers for a struct.
///
/// The struct must specify `#[rusqlite(table = "...")]` and mark at least one
/// field with `#[rusqlite(key)]`. It must also have at least one insertable field
/// and one updatable non-key field. Values are bound through `ToSql` for
/// `insert`, key-based `update`, and `upsert` operations.
#[proc_macro_derive(RusqliteWrite, attributes(rusqlite))]
pub fn derive_write(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let def = syn::parse_macro_input!(input as syn::DeriveInput);

    match write(def) {
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
    key: bool,
    #[attribute(conflicts = [skip_write])]
    skip_insert: bool,
    #[attribute(conflicts = [skip_write])]
    skip_update: bool,
    #[attribute(conflicts = [skip_insert, skip_update])]
    skip_write: bool,
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
        .unwrap_or_else(|| name.unraw().to_string());

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
                        .unwrap_or_else(|| field_name.unraw().to_string());
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

struct WriteField {
    member: syn::Member,
    ty: syn::Type,
    column: String,
    key: bool,
    skip_insert: bool,
    skip_update: bool,
    span: proc_macro2::Span,
}

fn write(input: syn::DeriveInput) -> syn::Result<proc_macro2::TokenStream> {
    let syn::Data::Struct(data) = input.data else {
        return Err(syn::Error::new_spanned(
            input.ident,
            "only structs are supported for now",
        ));
    };

    let name = input.ident;
    let mut generics = input.generics;
    let table_attr = RusqliteTable::from_attributes(&input.attrs)?;
    let table = table_attr.table.ok_or_else(|| {
        syn::Error::new_spanned(&name, "RusqliteWrite requires #[rusqlite(table = \"...\")]")
    })?;

    let mut write_fields = Vec::new();
    match data.fields {
        syn::Fields::Named(fields) => {
            for field in fields.named {
                let field_name = field
                    .ident
                    .clone()
                    .expect("fields were checked to be named");
                let attr = RusqliteField::from_attributes(&field.attrs)?;
                validate_skip_combination(&field, &attr)?;

                if attr.skip_write {
                    continue;
                }

                let column = match attr.column {
                    Some(column) => column,
                    None if attr.select.is_none() => field_name.unraw().to_string(),
                    None if attr.key => {
                        return Err(syn::Error::new_spanned(
                            &field,
                            "key field has no writable column; add #[rusqlite(column = \"...\")]",
                        ));
                    }
                    None => {
                        return Err(syn::Error::new_spanned(
                            &field,
                            "field has a read expression but no writable column; add #[rusqlite(column = \"...\")] or #[rusqlite(skip_write)]",
                        ));
                    }
                };

                let span = field.span();
                write_fields.push(WriteField {
                    member: syn::Member::Named(field_name),
                    ty: field.ty,
                    column,
                    key: attr.key,
                    skip_insert: attr.skip_insert,
                    skip_update: attr.skip_update,
                    span,
                });
            }
        }
        syn::Fields::Unnamed(fields) => {
            for (index, field) in fields.unnamed.into_iter().enumerate() {
                let attr = RusqliteField::from_attributes(&field.attrs)?;
                validate_skip_combination(&field, &attr)?;

                if attr.skip_write {
                    continue;
                }

                let column = attr.column.ok_or_else(|| {
                    let message = if attr.key {
                        "key tuple field has no writable column; add #[rusqlite(column = \"...\")]"
                    } else {
                        "tuple field has no writable column; add #[rusqlite(column = \"...\")] or #[rusqlite(skip_write)]"
                    };
                    syn::Error::new_spanned(&field, message)
                })?;

                let span = field.span();
                write_fields.push(WriteField {
                    member: syn::Member::Unnamed(syn::Index::from(index)),
                    ty: field.ty,
                    column,
                    key: attr.key,
                    skip_insert: attr.skip_insert,
                    skip_update: attr.skip_update,
                    span,
                });
            }
        }
        syn::Fields::Unit => {}
    }

    for (index, field) in write_fields.iter().enumerate() {
        if write_fields[..index]
            .iter()
            .any(|previous| previous.column == field.column)
        {
            return Err(syn::Error::new(
                field.span,
                format!("duplicate writable column `{}`", field.column),
            ));
        }
    }

    let keys: Vec<_> = write_fields.iter().filter(|field| field.key).collect();
    if keys.is_empty() {
        return Err(syn::Error::new_spanned(
            &name,
            "RusqliteWrite requires at least one #[rusqlite(key)] field",
        ));
    }

    let insert_fields: Vec<_> = write_fields
        .iter()
        .filter(|field| !field.skip_insert)
        .collect();
    if insert_fields.is_empty() {
        return Err(syn::Error::new_spanned(
            &name,
            "RusqliteWrite requires at least one insertable field",
        ));
    }

    let update_fields: Vec<_> = write_fields
        .iter()
        .filter(|field| !field.key && !field.skip_update)
        .collect();
    if update_fields.is_empty() {
        return Err(syn::Error::new_spanned(
            &name,
            "RusqliteWrite requires at least one updatable non-key field",
        ));
    }

    for field in &write_fields {
        add_field_bound(
            &mut generics,
            &field.ty,
            quote::quote!(::rusqlite::types::ToSql),
        );
    }

    let insert_columns = insert_fields
        .iter()
        .map(|field| field.column.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    let insert_placeholders = (1..=insert_fields.len())
        .map(|index| format!("?{index}"))
        .collect::<Vec<_>>()
        .join(", ");
    let insert_query =
        format!("INSERT INTO {table} ({insert_columns}) VALUES ({insert_placeholders});");

    let update_assignments = update_fields
        .iter()
        .enumerate()
        .map(|(index, field)| format!("{} = ?{}", field.column, index + 1))
        .collect::<Vec<_>>()
        .join(", ");
    let update_predicate = keys
        .iter()
        .enumerate()
        .map(|(index, field)| format!("{} = ?{}", field.column, update_fields.len() + index + 1))
        .collect::<Vec<_>>()
        .join(" AND ");
    let update_query = format!("UPDATE {table} SET {update_assignments} WHERE {update_predicate};");

    let conflict_columns = keys
        .iter()
        .map(|field| field.column.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    // Fields present in the insert can use `excluded`. An updatable field that
    // is skipped during insertion needs a separate binding on the conflict
    // path; `excluded.column` would contain its database default rather than
    // the value from `self`.
    let separately_bound_upsert_fields: Vec<_> = update_fields
        .iter()
        .filter(|field| field.skip_insert)
        .copied()
        .collect();
    let mut next_upsert_placeholder = insert_fields.len() + 1;
    let upsert_assignments = update_fields
        .iter()
        .map(|field| {
            if field.skip_insert {
                let placeholder = next_upsert_placeholder;
                next_upsert_placeholder += 1;
                format!("{} = ?{placeholder}", field.column)
            } else {
                format!("{} = excluded.{}", field.column, field.column)
            }
        })
        .collect::<Vec<_>>()
        .join(", ");
    let upsert_query = format!(
        "INSERT INTO {table} ({insert_columns}) VALUES ({insert_placeholders}) ON CONFLICT ({conflict_columns}) DO UPDATE SET {upsert_assignments};"
    );

    let insert_values = insert_fields.iter().map(|field| field_value(field));
    let update_values = update_fields
        .iter()
        .chain(keys.iter())
        .map(|field| field_value(field));
    let upsert_values = insert_fields
        .iter()
        .chain(separately_bound_upsert_fields.iter())
        .map(|field| field_value(field));
    let (impl_generics, type_generics, where_clause) = generics.split_for_impl();

    Ok(quote::quote! {
        impl #impl_generics ::rusqlite_derive::RusqliteWrite for #name #type_generics #where_clause {
            fn insert(&self, conn: &::rusqlite::Connection) -> ::rusqlite::Result<usize> {
                conn.execute(#insert_query, ::rusqlite::params![#(#insert_values),*])
            }

            fn update(&self, conn: &::rusqlite::Connection) -> ::rusqlite::Result<usize> {
                conn.execute(#update_query, ::rusqlite::params![#(#update_values),*])
            }

            fn upsert(&self, conn: &::rusqlite::Connection) -> ::rusqlite::Result<usize> {
                conn.execute(#upsert_query, ::rusqlite::params![#(#upsert_values),*])
            }
        }
    })
}

fn validate_skip_combination(field: &syn::Field, attr: &RusqliteField) -> syn::Result<()> {
    if attr.key && attr.skip_write {
        return Err(syn::Error::new_spanned(
            field,
            "key fields participate in update predicates; for a database-generated key, use #[rusqlite(key, skip_insert)] instead of skip_write",
        ));
    }
    if attr.key && attr.skip_update {
        return Err(syn::Error::new_spanned(
            field,
            "key fields are already excluded from update assignments; remove #[rusqlite(skip_update)]",
        ));
    }
    if !attr.key && attr.skip_insert && attr.skip_update {
        return Err(syn::Error::new_spanned(
            field,
            "a field skipped for both insert and update should use #[rusqlite(skip_write)]",
        ));
    }
    Ok(())
}

fn field_value(field: &WriteField) -> proc_macro2::TokenStream {
    let member = &field.member;
    let ty = &field.ty;
    quote::quote_spanned! { ty.span() =>
        &self.#member as &dyn ::rusqlite::types::ToSql
    }
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
