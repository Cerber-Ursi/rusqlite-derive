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
/// read expression, `read_default` omits a field from reads and initializes it
/// with `Default::default()`, and `aggregate` collects a selected value across
/// rows with equal non-aggregated fields. `aggregate(item = Type)` specifies
/// the `FromIterator` item type when Rust cannot infer it.
///
/// # Renaming the dependency
///
/// `#[rusqlite(crate = "...")]` overrides the wrapper crate path when its
/// dependency has been renamed.
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
/// field with `#[rusqlite(key)]`. The struct must also have at least one
/// insertable field and one updatable non-key field. Values are bound through
/// `ToSql` for `insert`, key-based `update`, and `upsert` operations.
///
/// # Renaming the dependency
///
/// `#[rusqlite(crate = "...")]` overrides the wrapper crate path when its
/// dependency has been renamed.
#[proc_macro_derive(RusqliteWrite, attributes(rusqlite))]
pub fn derive_write(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let def = syn::parse_macro_input!(input as syn::DeriveInput);

    match write(def) {
        Ok(ts) => ts,
        Err(e) => e.into_compile_error(),
    }
    .into()
}

struct RusqliteTable {
    from: Option<String>,
    table: Option<String>,
    crate_path: Option<syn::Path>,
}

impl RusqliteTable {
    fn from_attributes(attributes: &[syn::Attribute]) -> syn::Result<Self> {
        let mut from = None;
        let mut table = None;
        let mut crate_path = None;
        let mut errors: Option<syn::Error> = None;

        for attribute in attributes
            .iter()
            .filter(|attribute| attribute.path().is_ident("rusqlite"))
        {
            attribute.parse_nested_meta(|meta| {
                let name = if meta.path.is_ident("from") {
                    "from"
                } else if meta.path.is_ident("table") {
                    "table"
                } else if meta.path.is_ident("crate") {
                    "crate"
                } else {
                    return Err(meta.error("supported fields are `from`, `table`, and `crate`"));
                };
                let value = meta.value()?.parse::<syn::LitStr>()?;

                match name {
                    "from" => set_once(&mut from, value.value(), &value, name, &mut errors),
                    "table" => set_once(&mut table, value.value(), &value, name, &mut errors),
                    "crate" => {
                        let path = value.parse::<syn::Path>()?;
                        set_once(&mut crate_path, path, &value, name, &mut errors);
                    }
                    _ => unreachable!("all supported attributes were matched"),
                }
                Ok(())
            })?;
        }

        if let Some(error) = errors {
            return Err(error);
        }

        Ok(Self {
            from: from.map(|(value, _)| value),
            table: table.map(|(value, _)| value),
            crate_path: crate_path.map(|(value, _)| value),
        })
    }
}

fn set_once<T>(
    slot: &mut Option<(T, proc_macro2::Span)>,
    value: T,
    value_literal: &syn::LitStr,
    name: &str,
    errors: &mut Option<syn::Error>,
) {
    if let Some((_, first_span)) = slot {
        let message = format!("`{name}` is specified multiple times");
        let mut error = syn::Error::new(*first_span, &message);
        error.combine(syn::Error::new(value_literal.span(), message));
        if let Some(errors) = errors {
            errors.combine(error);
        } else {
            *errors = Some(error);
        }
    } else {
        *slot = Some((value, value_literal.span()));
    }
}

#[derive(FromAttr)]
#[attribute(ident = rusqlite)]
struct RusqliteField {
    #[attribute(conflicts = [read_default])]
    select: Option<String>,
    column: Option<String>,
    #[attribute(conflicts = [select, aggregate])]
    read_default: bool,
    #[attribute(conflicts = [read_default])]
    aggregate: bool,
    item: Option<syn::Type>,
    key: bool,
    #[attribute(conflicts = [skip_write])]
    skip_insert: bool,
    #[attribute(conflicts = [skip_write])]
    skip_update: bool,
    #[attribute(conflicts = [skip_insert, skip_update])]
    skip_write: bool,
}

fn field_attributes(attributes: &[syn::Attribute]) -> syn::Result<RusqliteField> {
    let attributes: Vec<_> = attributes
        .iter()
        .cloned()
        .map(|mut attribute| {
            if attribute.path().is_ident("rusqlite") {
                if let syn::Meta::List(list) = &mut attribute.meta {
                    list.tokens = flatten_aggregate_options(list.tokens.clone());
                }
            }
            attribute
        })
        .collect();
    RusqliteField::from_attributes(&attributes)
}

fn flatten_aggregate_options(tokens: proc_macro2::TokenStream) -> proc_macro2::TokenStream {
    let mut tokens = tokens.into_iter().peekable();
    let mut flattened = proc_macro2::TokenStream::new();

    while let Some(token) = tokens.next() {
        let aggregate =
            matches!(&token, proc_macro2::TokenTree::Ident(ident) if ident == "aggregate");
        flattened.extend(::core::iter::once(token));

        if aggregate {
            if let Some(proc_macro2::TokenTree::Group(group)) = tokens.peek() {
                if group.delimiter() == proc_macro2::Delimiter::Parenthesis {
                    let group = match tokens.next() {
                        Some(proc_macro2::TokenTree::Group(group)) => group,
                        _ => unreachable!("peeked token was a group"),
                    };
                    if !group.stream().is_empty() {
                        let mut comma = proc_macro2::Punct::new(',', proc_macro2::Spacing::Alone);
                        comma.set_span(group.span());
                        flattened.extend([proc_macro2::TokenTree::Punct(comma)]);
                        flattened.extend(group.stream());
                    }
                }
            }
        }
    }

    flattened
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
    let crate_path = table_attr
        .crate_path
        .unwrap_or_else(|| syn::parse_quote!(::rusqlite_derive));

    let mut identity_fields = Vec::new();
    let mut aggregate_fields = Vec::new();
    let mut row_values = Vec::new();
    let mut group_types = Vec::new();
    let mut new_group_values = Vec::new();
    let (columns, row_value, grouped_value) = match data.fields {
        syn::Fields::Named(fields) => {
            let mut columns = vec![];
            let mut values = vec![];
            let mut grouped_values = vec![];

            for field in fields.named {
                let field_name = field
                    .ident
                    .as_ref()
                    .expect("fields were checked to be named");
                let attr = field_attributes(&field.attrs)?;
                let ty = &field.ty;
                if attr.item.is_some() && !attr.aggregate {
                    return Err(syn::Error::new_spanned(
                        &field,
                        "`item` requires `aggregate`",
                    ));
                }

                if attr.read_default {
                    add_field_bound(&mut generics, ty, quote::quote!(::core::default::Default));
                    let value = quote::quote_spanned! { ty.span() =>
                        ::core::default::Default::default()
                    };
                    values.push(quote::quote! { #field_name: #value, });
                    grouped_values.push(quote::quote! { #field_name: #value, });
                } else {
                    let index = columns.len();
                    let group_index = syn::Index::from(row_values.len());
                    let column = attr
                        .select
                        .or(attr.column)
                        .unwrap_or_else(|| field_name.unraw().to_string());
                    columns.push(column);

                    if attr.aggregate {
                        let item = if let Some(item) = attr.item {
                            add_field_bound(
                                &mut generics,
                                ty,
                                quote::quote!(::core::iter::FromIterator<#item>),
                            );
                            add_field_bound(
                                &mut generics,
                                &item,
                                quote::quote!(#crate_path::rusqlite::types::FromSql),
                            );
                            quote::quote!(#item)
                        } else {
                            quote::quote!(_)
                        };
                        row_values.push(quote::quote! {
                            row.get::<_, #crate_path::rusqlite::types::Value>(#index)?
                        });
                        group_types.push(quote::quote! {
                            ::std::vec::Vec<#crate_path::rusqlite::types::Value>
                        });
                        new_group_values.push(quote::quote! {
                            ::std::vec![value.#group_index]
                        });
                        let value = quote::quote_spanned! { ty.span() =>
                            #crate_path::__private_collect_aggregate_or_use_item_attribute::<#ty, #item>(
                                group.#group_index,
                                #index,
                                &aggregate_column_names[#index],
                            )?
                        };
                        values.push(quote::quote! { #field_name: #value, });
                        grouped_values.push(quote::quote! { #field_name: #value, });
                        aggregate_fields.push(group_index);
                    } else {
                        add_field_bound(
                            &mut generics,
                            ty,
                            quote::quote!(#crate_path::rusqlite::types::FromSql),
                        );
                        let value = quote::quote_spanned! { ty.span() =>
                            row.get::<_, #ty>(#index)?
                        };
                        values.push(quote::quote! { #field_name: #value, });
                        row_values.push(value.clone());
                        group_types.push(quote::quote! { #ty });
                        new_group_values.push(quote::quote! { value.#group_index });
                        grouped_values.push(quote::quote! { #field_name: group.#group_index, });
                        identity_fields.push((group_index, field.ty));
                    }
                }
            }

            (
                columns,
                quote::quote! { #name { #(#values)* } },
                quote::quote! { #name { #(#grouped_values)* } },
            )
        }
        syn::Fields::Unnamed(fields) => {
            let mut columns = vec![];
            let mut values = vec![];
            let mut grouped_values = vec![];

            for field in fields.unnamed {
                let attr = field_attributes(&field.attrs)?;
                let ty = &field.ty;
                if attr.item.is_some() && !attr.aggregate {
                    return Err(syn::Error::new_spanned(
                        &field,
                        "`item` requires `aggregate`",
                    ));
                }

                if attr.read_default {
                    add_field_bound(&mut generics, ty, quote::quote!(::core::default::Default));
                    let value = quote::quote_spanned! { ty.span() =>
                        ::core::default::Default::default(),
                    };
                    values.push(value.clone());
                    grouped_values.push(value);
                } else {
                    let column = attr.select.or(attr.column).ok_or_else(|| {
                        syn::Error::new_spanned(
                            &field,
                            "tuple struct fields require #[rusqlite(select = \"...\")], #[rusqlite(column = \"...\")], or #[rusqlite(read_default)]",
                        )
                    })?;
                    let index = columns.len();
                    let group_index = syn::Index::from(row_values.len());
                    columns.push(column);

                    if attr.aggregate {
                        let item = if let Some(item) = attr.item {
                            add_field_bound(
                                &mut generics,
                                ty,
                                quote::quote!(::core::iter::FromIterator<#item>),
                            );
                            add_field_bound(
                                &mut generics,
                                &item,
                                quote::quote!(#crate_path::rusqlite::types::FromSql),
                            );
                            quote::quote!(#item)
                        } else {
                            quote::quote!(_)
                        };
                        row_values.push(quote::quote! {
                            row.get::<_, #crate_path::rusqlite::types::Value>(#index)?
                        });
                        group_types.push(quote::quote! {
                            ::std::vec::Vec<#crate_path::rusqlite::types::Value>
                        });
                        new_group_values.push(quote::quote! {
                            ::std::vec![value.#group_index]
                        });
                        let value = quote::quote_spanned! { ty.span() =>
                            #crate_path::__private_collect_aggregate_or_use_item_attribute::<#ty, #item>(
                                group.#group_index,
                                #index,
                                &aggregate_column_names[#index],
                            )?,
                        };
                        values.push(value.clone());
                        grouped_values.push(value);
                        aggregate_fields.push(group_index);
                    } else {
                        add_field_bound(
                            &mut generics,
                            ty,
                            quote::quote!(#crate_path::rusqlite::types::FromSql),
                        );
                        let value = quote::quote_spanned! { ty.span() =>
                            row.get::<_, #ty>(#index)?
                        };
                        values.push(quote::quote! { #value, });
                        row_values.push(value);
                        group_types.push(quote::quote! { #ty });
                        new_group_values.push(quote::quote! { value.#group_index });
                        grouped_values.push(quote::quote! { group.#group_index, });
                        identity_fields.push((group_index, field.ty));
                    }
                }
            }

            (
                columns,
                quote::quote! { #name(#(#values)*) },
                quote::quote! { #name(#(#grouped_values)*) },
            )
        }
        syn::Fields::Unit => (vec![], quote::quote! { #name }, quote::quote! { #name }),
    };

    // SQLite still needs a result expression when every field is defaulted.
    // Selecting a constant preserves one constructed value per source row.
    let select = if columns.is_empty() {
        "1".to_owned()
    } else {
        columns.join(", ")
    };
    let query_simple = format!("SELECT {select} FROM {source};");
    let query_with_where = format!("SELECT {select} FROM {source} WHERE ");

    let aggregate = !aggregate_fields.is_empty();
    if aggregate {
        for (_, ty) in &identity_fields {
            add_field_bound(&mut generics, ty, quote::quote!(::core::cmp::PartialEq));
        }
    }

    let (impl_generics, type_generics, where_clause) = generics.split_for_impl();

    if aggregate {
        let comparisons = identity_fields.iter().map(|(index, _)| {
            quote::quote! {
                ::core::cmp::PartialEq::eq(&existing.#index, &value.#index)
            }
        });
        let equality = quote::quote! {
            true #(&& #comparisons)*
        };
        let merge = aggregate_fields.iter().map(|index| {
            quote::quote! {
                existing.#index.push(value.#index);
            }
        });
        let merge: Vec<_> = merge.collect();
        let collect_rows = quote::quote! {
            let mut values: ::std::vec::Vec<(#(#group_types,)*)> =
                ::std::vec::Vec::new();
            while let Some(row) = rows.next()? {
                let value = (#(#row_values,)*);
                if let Some(existing) = values.iter_mut().find(|existing| #equality) {
                    #(#merge)*
                } else {
                    values.push((#(#new_group_values,)*));
                }
            }
            values
                .into_iter()
                .map(|group| -> #crate_path::rusqlite::Result<Self> {
                    Ok(#grouped_value)
                })
                .collect()
        };

        Ok(quote::quote! {
            impl #impl_generics #crate_path::RusqliteFetch for #name #type_generics #where_clause {
                fn fetch(conn: &#crate_path::rusqlite::Connection) -> #crate_path::rusqlite::Result<Vec<Self>> {
                    let mut statement = conn.prepare(#query_simple)?;
                    let aggregate_column_names: Vec<::std::string::String> = statement
                        .column_names()
                        .into_iter()
                        .map(::std::borrow::ToOwned::to_owned)
                        .collect();
                    let mut rows = statement.query([])?;
                    #collect_rows
                }

                fn fetch_with_filter<P: #crate_path::rusqlite::Params>(
                    conn: &#crate_path::rusqlite::Connection,
                    filter: &str,
                    params: P,
                ) -> #crate_path::rusqlite::Result<Vec<Self>> {
                    let mut query = ::std::string::String::from(#query_with_where);
                    query.push_str(filter);
                    query.push(';');
                    let mut statement = conn.prepare(&query)?;
                    let aggregate_column_names: Vec<::std::string::String> = statement
                        .column_names()
                        .into_iter()
                        .map(::std::borrow::ToOwned::to_owned)
                        .collect();
                    let mut rows = statement.query(params)?;
                    #collect_rows
                }
            }
        })
    } else {
        Ok(quote::quote! {
            impl #impl_generics #crate_path::RusqliteFetch for #name #type_generics #where_clause {
                fn fetch(conn: &#crate_path::rusqlite::Connection) -> #crate_path::rusqlite::Result<Vec<Self>> {
                    conn
                        .prepare(#query_simple)?
                        .query_map([], |row| {
                            Ok(#row_value)
                        })?
                        .collect()
                }

                fn fetch_with_filter<P: #crate_path::rusqlite::Params>(
                    conn: &#crate_path::rusqlite::Connection,
                    filter: &str,
                    params: P,
                ) -> #crate_path::rusqlite::Result<Vec<Self>> {
                    let mut query = ::std::string::String::from(#query_with_where);
                    query.push_str(filter);
                    query.push(';');
                    conn
                        .prepare(&query)?
                        .query_map(params, |row| {
                            Ok(#row_value)
                        })?
                        .collect()
                }
            }
        })
    }
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
    let crate_path = table_attr
        .crate_path
        .unwrap_or_else(|| syn::parse_quote!(::rusqlite_derive));

    let mut write_fields = Vec::new();
    match data.fields {
        syn::Fields::Named(fields) => {
            for field in fields.named {
                let field_name = field
                    .ident
                    .clone()
                    .expect("fields were checked to be named");
                let attr = field_attributes(&field.attrs)?;
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
                let attr = field_attributes(&field.attrs)?;
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
            quote::quote!(#crate_path::rusqlite::types::ToSql),
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

    let insert_values = insert_fields
        .iter()
        .map(|field| field_value(field, &crate_path));
    let update_values = update_fields
        .iter()
        .chain(keys.iter())
        .map(|field| field_value(field, &crate_path));
    let upsert_values = insert_fields
        .iter()
        .chain(separately_bound_upsert_fields.iter())
        .map(|field| field_value(field, &crate_path));
    let (impl_generics, type_generics, where_clause) = generics.split_for_impl();

    Ok(quote::quote! {
        impl #impl_generics #crate_path::RusqliteWrite for #name #type_generics #where_clause {
            fn insert(&self, conn: &#crate_path::rusqlite::Connection) -> #crate_path::rusqlite::Result<usize> {
                conn.execute(#insert_query, #crate_path::rusqlite::params![#(#insert_values),*])
            }

            fn update(&self, conn: &#crate_path::rusqlite::Connection) -> #crate_path::rusqlite::Result<usize> {
                conn.execute(#update_query, #crate_path::rusqlite::params![#(#update_values),*])
            }

            fn upsert(&self, conn: &#crate_path::rusqlite::Connection) -> #crate_path::rusqlite::Result<usize> {
                conn.execute(#upsert_query, #crate_path::rusqlite::params![#(#upsert_values),*])
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

fn field_value(field: &WriteField, crate_path: &syn::Path) -> proc_macro2::TokenStream {
    let member = &field.member;
    let ty = &field.ty;
    quote::quote_spanned! { ty.span() =>
        &self.#member as &dyn #crate_path::rusqlite::types::ToSql
    }
}

fn add_field_bound(
    generics: &mut syn::Generics,
    ty: &syn::Type,
    trait_path: proc_macro2::TokenStream,
) {
    // A concrete field is checked at the generated operation whose span is the
    // field type. Only generic-dependent fields need an impl bound.
    if !field_uses_generic(generics, ty) {
        return;
    }

    let predicate = syn::parse_quote_spanned! {ty.span()=> #ty: #trait_path};
    generics.make_where_clause().predicates.push(predicate);
}

fn field_uses_generic(generics: &syn::Generics, ty: &syn::Type) -> bool {
    use quote::ToTokens;

    let parameter_names: Vec<_> = generics
        .params
        .iter()
        .map(|parameter| match parameter {
            syn::GenericParam::Type(parameter) => parameter.ident.to_string(),
            syn::GenericParam::Lifetime(parameter) => parameter.lifetime.ident.to_string(),
            syn::GenericParam::Const(parameter) => parameter.ident.to_string(),
        })
        .collect();
    contains_parameter(ty.to_token_stream(), &parameter_names)
}

fn contains_parameter(tokens: proc_macro2::TokenStream, parameters: &[String]) -> bool {
    tokens.into_iter().any(|token| match token {
        proc_macro2::TokenTree::Ident(ident) => parameters.contains(&ident.to_string()),
        proc_macro2::TokenTree::Group(group) => contains_parameter(group.stream(), parameters),
        _ => false,
    })
}
