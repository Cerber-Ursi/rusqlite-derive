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

#[derive(Clone, Copy)]
enum StructKind {
    Named,
    Unnamed,
    Unit,
}

struct FetchMapping {
    columns: Vec<String>,
    row_value: proc_macro2::TokenStream,
    grouped_value: proc_macro2::TokenStream,
    identity_fields: Vec<(syn::Index, syn::Type)>,
    aggregate_fields: Vec<syn::Index>,
    row_values: Vec<proc_macro2::TokenStream>,
    group_types: Vec<proc_macro2::TokenStream>,
    new_group_values: Vec<proc_macro2::TokenStream>,
}

struct FetchField {
    name: Option<syn::Ident>,
    row_value: proc_macro2::TokenStream,
    grouped_value: proc_macro2::TokenStream,
}

struct FetchMappingBuilder<'a> {
    generics: &'a mut syn::Generics,
    crate_path: &'a syn::Path,
    columns: Vec<String>,
    fields: Vec<FetchField>,
    identity_fields: Vec<(syn::Index, syn::Type)>,
    aggregate_fields: Vec<syn::Index>,
    row_values: Vec<proc_macro2::TokenStream>,
    group_types: Vec<proc_macro2::TokenStream>,
    new_group_values: Vec<proc_macro2::TokenStream>,
}

impl<'a> FetchMappingBuilder<'a> {
    fn new(generics: &'a mut syn::Generics, crate_path: &'a syn::Path) -> Self {
        Self {
            generics,
            crate_path,
            columns: Vec::new(),
            fields: Vec::new(),
            identity_fields: Vec::new(),
            aggregate_fields: Vec::new(),
            row_values: Vec::new(),
            group_types: Vec::new(),
            new_group_values: Vec::new(),
        }
    }

    fn add_field(&mut self, field: syn::Field, name: Option<syn::Ident>) -> syn::Result<()> {
        let attr = field_attributes(&field.attrs)?;
        if attr.item.is_some() && !attr.aggregate {
            return Err(syn::Error::new_spanned(
                &field,
                "`item` requires `aggregate`",
            ));
        }

        let ty = field.ty.clone();
        if attr.read_default {
            add_field_bound(self.generics, &ty, quote::quote!(::core::default::Default));
            let value = quote::quote_spanned! { ty.span() =>
                ::core::default::Default::default()
            };
            self.fields.push(FetchField {
                name,
                row_value: value.clone(),
                grouped_value: value,
            });
            return Ok(());
        }

        let column = attr
            .select
            .or(attr.column)
            .or_else(|| name.as_ref().map(|name| name.unraw().to_string()))
            .ok_or_else(|| {
                syn::Error::new_spanned(
                    &field,
                    "tuple struct fields require #[rusqlite(select = \"...\")], #[rusqlite(column = \"...\")], or #[rusqlite(read_default)]",
                )
            })?;
        let column_index = self.columns.len();
        let group_index = syn::Index::from(self.row_values.len());
        self.columns.push(column);

        let crate_path = self.crate_path;
        let (row_value, grouped_value) = if attr.aggregate {
            let item = aggregate_item_type(self.generics, &ty, attr.item, crate_path);
            self.row_values.push(quote::quote! {
                row.get::<_, #crate_path::rusqlite::types::Value>(#column_index)?
            });
            self.group_types.push(quote::quote! {
                ::std::vec::Vec<#crate_path::rusqlite::types::Value>
            });
            self.new_group_values.push(quote::quote! {
                ::std::vec![value.#group_index]
            });
            self.aggregate_fields.push(group_index.clone());
            let value = quote::quote_spanned! { ty.span() =>
                #crate_path::__private_collect_aggregate_or_use_item_attribute::<#ty, #item>(
                    group.#group_index,
                    #column_index,
                    &aggregate_column_names[#column_index],
                )?
            };
            (value.clone(), value)
        } else {
            add_field_bound(
                self.generics,
                &ty,
                quote::quote!(#crate_path::rusqlite::types::FromSql),
            );
            let value = quote::quote_spanned! { ty.span() =>
                row.get::<_, #ty>(#column_index)?
            };
            self.row_values.push(value.clone());
            self.group_types.push(quote::quote! { #ty });
            self.new_group_values
                .push(quote::quote! { value.#group_index });
            self.identity_fields.push((group_index.clone(), ty));
            (value, quote::quote! { group.#group_index })
        };
        self.fields.push(FetchField {
            name,
            row_value,
            grouped_value,
        });
        Ok(())
    }

    fn finish(self, name: &syn::Ident, kind: StructKind) -> FetchMapping {
        let row_values = self.fields.iter().map(|field| &field.row_value);
        let grouped_values = self.fields.iter().map(|field| &field.grouped_value);
        let row_value = construct_struct(name, kind, &self.fields, row_values);
        let grouped_value = construct_struct(name, kind, &self.fields, grouped_values);

        FetchMapping {
            columns: self.columns,
            row_value,
            grouped_value,
            identity_fields: self.identity_fields,
            aggregate_fields: self.aggregate_fields,
            row_values: self.row_values,
            group_types: self.group_types,
            new_group_values: self.new_group_values,
        }
    }
}

fn aggregate_item_type(
    generics: &mut syn::Generics,
    collection: &syn::Type,
    item: Option<syn::Type>,
    crate_path: &syn::Path,
) -> proc_macro2::TokenStream {
    let Some(item) = item else {
        return quote::quote!(_);
    };

    add_field_bound(
        generics,
        collection,
        quote::quote!(::core::iter::FromIterator<#item>),
    );
    add_field_bound(
        generics,
        &item,
        quote::quote!(#crate_path::rusqlite::types::FromSql),
    );
    quote::quote!(#item)
}

fn construct_struct<'a>(
    name: &syn::Ident,
    kind: StructKind,
    fields: &[FetchField],
    values: impl Iterator<Item = &'a proc_macro2::TokenStream>,
) -> proc_macro2::TokenStream {
    match kind {
        StructKind::Named => {
            let names = fields
                .iter()
                .map(|field| field.name.as_ref().expect("named struct fields have names"));
            quote::quote! { #name { #(#names: #values,)* } }
        }
        StructKind::Unnamed => quote::quote! { #name(#(#values,)*) },
        StructKind::Unit => quote::quote! { #name },
    }
}

fn build_fetch_mapping(
    fields: syn::Fields,
    name: &syn::Ident,
    generics: &mut syn::Generics,
    crate_path: &syn::Path,
) -> syn::Result<FetchMapping> {
    let mut builder = FetchMappingBuilder::new(generics, crate_path);
    let kind = match fields {
        syn::Fields::Named(fields) => {
            for field in fields.named {
                let field_name = field
                    .ident
                    .clone()
                    .expect("fields were checked to be named");
                builder.add_field(field, Some(field_name))?;
            }
            StructKind::Named
        }
        syn::Fields::Unnamed(fields) => {
            for field in fields.unnamed {
                builder.add_field(field, None)?;
            }
            StructKind::Unnamed
        }
        syn::Fields::Unit => StructKind::Unit,
    };
    Ok(builder.finish(name, kind))
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

    let FetchMapping {
        columns,
        row_value,
        grouped_value,
        identity_fields,
        aggregate_fields,
        row_values,
        group_types,
        new_group_values,
    } = build_fetch_mapping(data.fields, &name, &mut generics, &crate_path)?;

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
    let (prepare_rows, collect_rows) = if aggregate {
        for (_, ty) in &identity_fields {
            add_field_bound(&mut generics, ty, quote::quote!(::core::cmp::PartialEq));
        }

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
        (
            quote::quote! {
                let aggregate_column_names: Vec<::std::string::String> = statement
                    .column_names()
                    .into_iter()
                    .map(::std::borrow::ToOwned::to_owned)
                    .collect();
            },
            quote::quote! {
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
            },
        )
    } else {
        (
            proc_macro2::TokenStream::new(),
            quote::quote! {
                let mut values = ::std::vec::Vec::new();
                while let Some(row) = rows.next()? {
                    values.push(#row_value);
                }
                Ok(values)
            },
        )
    };

    let (impl_generics, type_generics, where_clause) = generics.split_for_impl();
    Ok(quote::quote! {
        impl #impl_generics #crate_path::RusqliteFetch for #name #type_generics #where_clause {
            fn fetch(conn: &#crate_path::rusqlite::Connection) -> #crate_path::rusqlite::Result<Vec<Self>> {
                let mut statement = conn.prepare(#query_simple)?;
                #prepare_rows
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
                #prepare_rows
                let mut rows = statement.query(params)?;
                #collect_rows
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

fn collect_write_fields(fields: syn::Fields) -> syn::Result<Vec<WriteField>> {
    let mut write_fields = Vec::new();
    match fields {
        syn::Fields::Named(fields) => {
            for field in fields.named {
                let name = field
                    .ident
                    .clone()
                    .expect("fields were checked to be named");
                let column = name.unraw().to_string();
                if let Some(field) = write_field(field, syn::Member::Named(name), Some(column))? {
                    write_fields.push(field);
                }
            }
        }
        syn::Fields::Unnamed(fields) => {
            for (index, field) in fields.unnamed.into_iter().enumerate() {
                if let Some(field) =
                    write_field(field, syn::Member::Unnamed(syn::Index::from(index)), None)?
                {
                    write_fields.push(field);
                }
            }
        }
        syn::Fields::Unit => {}
    }
    Ok(write_fields)
}

fn write_field(
    field: syn::Field,
    member: syn::Member,
    implicit_column: Option<String>,
) -> syn::Result<Option<WriteField>> {
    let attr = field_attributes(&field.attrs)?;
    validate_skip_combination(&field, &attr)?;
    if attr.skip_write {
        return Ok(None);
    }

    let column = if let Some(column) = attr.column {
        column
    } else if let Some(implicit_column) = implicit_column {
        if attr.select.is_none() {
            implicit_column
        } else if attr.key {
            return Err(syn::Error::new_spanned(
                &field,
                "key field has no writable column; add #[rusqlite(column = \"...\")]",
            ));
        } else {
            return Err(syn::Error::new_spanned(
                &field,
                "field has a read expression but no writable column; add #[rusqlite(column = \"...\")] or #[rusqlite(skip_write)]",
            ));
        }
    } else {
        let message = if attr.key {
            "key tuple field has no writable column; add #[rusqlite(column = \"...\")]"
        } else {
            "tuple field has no writable column; add #[rusqlite(column = \"...\")] or #[rusqlite(skip_write)]"
        };
        return Err(syn::Error::new_spanned(&field, message));
    };

    let span = field.span();
    Ok(Some(WriteField {
        member,
        ty: field.ty,
        column,
        key: attr.key,
        skip_insert: attr.skip_insert,
        skip_update: attr.skip_update,
        span,
    }))
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

    let write_fields = collect_write_fields(data.fields)?;

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
