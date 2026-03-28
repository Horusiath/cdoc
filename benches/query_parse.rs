use cdoc::Query;
use criterion::{Criterion, criterion_group, criterion_main};

fn simple_single_field(c: &mut Criterion) {
    c.bench_function("query_parse/simple_single_field", |b| {
        b.iter(|| Query::parse("{ name }").unwrap());
    });
}

fn simple_multiple_fields(c: &mut Criterion) {
    c.bench_function("query_parse/simple_multiple_fields", |b| {
        b.iter(|| Query::parse("{ name, age, active, email, created_at }").unwrap());
    });
}

fn nested_two_levels(c: &mut Criterion) {
    c.bench_function("query_parse/nested_two_levels", |b| {
        b.iter(|| Query::parse("{ users { name, age } }").unwrap());
    });
}

fn complex_aliases_filters_subselects(c: &mut Criterion) {
    let input =
        r#"{ users { name as first_name, age, friends(skip: 1, take: 10) { name, email } } }"#;
    c.bench_function("query_parse/complex_aliases_filters_subselects", |b| {
        b.iter(|| Query::parse(input).unwrap());
    });
}

fn complex_deeply_nested(c: &mut Criterion) {
    let input = r#"{
        organization {
            departments(skip: 0, take: 50) {
                name as department_name,
                "head count",
                teams(after: "cursor_abc", take: 20) {
                    name as team_name,
                    members(skip: 5, take: 10, before: "cursor_xyz") {
                        "first name" as given_name,
                        "last name",
                        role,
                        profile {
                            avatar { url, width, height },
                            bio
                        }
                    }
                }
            }
        }
    }"#;
    c.bench_function("query_parse/complex_deeply_nested", |b| {
        b.iter(|| Query::parse(input).unwrap());
    });
}

fn complex_wide_many_selects(c: &mut Criterion) {
    let input = r#"{
        id,
        name as display_name,
        email,
        "phone number",
        active,
        role,
        created_at,
        updated_at,
        avatar { url, width, height },
        settings { theme, language, timezone },
        permissions(take: 100) { resource, level },
        tags(skip: 0, take: 50) { label, color }
    }"#;
    c.bench_function("query_parse/complex_wide_many_selects", |b| {
        b.iter(|| Query::parse(input).unwrap());
    });
}

criterion_group!(
    benches,
    simple_single_field,
    simple_multiple_fields,
    nested_two_levels,
    complex_aliases_filters_subselects,
    complex_deeply_nested,
    complex_wide_many_selects,
);
criterion_main!(benches);
