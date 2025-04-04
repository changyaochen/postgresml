use std::cmp::Ordering;
use std::collections::HashMap;
use std::fmt::{Display, Error, Formatter};
use std::str::FromStr;

use pgx::*;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::orm::Dataset;
use crate::orm::Sampling;
use crate::orm::Status;

#[derive(Debug, Eq, PartialEq, Serialize, Deserialize, Clone)]
pub struct Column {
    name: String,
    pg_type: String,
    nullable: bool,
    label: bool,
    position: usize,
    size: usize,
}

impl Column {
    fn quoted_name(&self) -> String {
        format!(r#""{}""#, self.name)
    }

    fn stats_safe_name(&self) -> String {
        match self.pg_type.as_str() {
            "bool" => self.quoted_name() + "::INT4",
            "bool[]" => self.quoted_name() + "::INT4[]",
            _ => self.quoted_name(),
        }
    }
}

impl PartialOrd for Column {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Column {
    fn cmp(&self, other: &Self) -> Ordering {
        self.position.cmp(&other.position)
    }
}

#[derive(Debug)]
pub struct Snapshot {
    pub id: i64,
    pub relation_name: String,
    pub y_column_name: Vec<String>,
    pub test_size: f32,
    pub test_sampling: Sampling,
    pub status: Status,
    pub columns: Option<JsonB>,
    pub analysis: Option<JsonB>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

impl Display for Snapshot {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        write!(f, "Snapshot {{ id: {}, relation_name: {}, y_column_name: {:?}, test_size: {}, test_sampling: {:?}, status: {:?} }}", self.id, self.relation_name, self.y_column_name, self.test_size, self.test_sampling, self.status)
    }
}

impl Snapshot {
    pub fn find_last_by_project_id(project_id: i64) -> Option<Snapshot> {
        let mut snapshot = None;
        Spi::connect(|client| {
            let result = client
                .select(
                    "SELECT
                        snapshots.id,
                        snapshots.relation_name,
                        snapshots.y_column_name,
                        snapshots.test_size,
                        snapshots.test_sampling::TEXT,
                        snapshots.status::TEXT,
                        snapshots.columns,
                        snapshots.analysis,
                        snapshots.created_at,
                        snapshots.updated_at 
                    FROM pgml.snapshots 
                    JOIN pgml.models
                      ON models.snapshot_id = snapshots.id
                      AND models.project_id = $1 
                    ORDER BY snapshots.id DESC 
                    LIMIT 1;
                    ",
                    Some(1),
                    Some(vec![(
                        PgBuiltInOids::INT8OID.oid(),
                        project_id.into_datum(),
                    )]),
                )
                .first();
            if !result.is_empty() {
                snapshot = Some(Snapshot {
                    id: result.get_datum(1).unwrap(),
                    relation_name: result.get_datum(2).unwrap(),
                    y_column_name: result.get_datum(3).unwrap(),
                    test_size: result.get_datum(4).unwrap(),
                    test_sampling: Sampling::from_str(result.get_datum(5).unwrap()).unwrap(),
                    status: Status::from_str(result.get_datum(6).unwrap()).unwrap(),
                    columns: result.get_datum(7),
                    analysis: result.get_datum(8),
                    created_at: result.get_datum(9).unwrap(),
                    updated_at: result.get_datum(10).unwrap(),
                });
            }
            Ok(Some(1))
        });
        snapshot
    }

    pub fn create(
        relation_name: &str,
        y_column_name: &str,
        test_size: f32,
        test_sampling: Sampling,
    ) -> Snapshot {
        let mut snapshot: Option<Snapshot> = None;
        let status = Status::in_progress;
        Spi::connect(|client| {
            let result = client.select("INSERT INTO pgml.snapshots (relation_name, y_column_name, test_size, test_sampling, status) VALUES ($1, $2, $3, $4::pgml.sampling, $5::pgml.status) RETURNING id, relation_name, y_column_name, test_size, test_sampling::TEXT, status::TEXT, columns, analysis, created_at, updated_at;",
                Some(1),
                Some(vec![
                    (PgBuiltInOids::TEXTOID.oid(), relation_name.into_datum()),
                    (PgBuiltInOids::TEXTARRAYOID.oid(), vec![y_column_name].into_datum()),
                    (PgBuiltInOids::FLOAT4OID.oid(), test_size.into_datum()),
                    (PgBuiltInOids::TEXTOID.oid(), test_sampling.to_string().into_datum()),
                    (PgBuiltInOids::TEXTOID.oid(), status.to_string().into_datum()),
                ])
            ).first();
            let mut s = Snapshot {
                id: result.get_datum(1).unwrap(),
                relation_name: result.get_datum(2).unwrap(),
                y_column_name: result.get_datum(3).unwrap(),
                test_size: result.get_datum(4).unwrap(),
                test_sampling: Sampling::from_str(result.get_datum(5).unwrap()).unwrap(),
                status,         // 6
                columns: None,  // 7
                analysis: None, // 8
                created_at: result.get_datum(9).unwrap(),
                updated_at: result.get_datum(10).unwrap(),
            };
            let mut sql = format!(
                r#"CREATE TABLE "pgml"."snapshot_{}" AS SELECT * FROM {}"#,
                s.id, s.relation_name
            );
            if s.test_sampling == Sampling::random {
                sql += " ORDER BY random()";
            }
            client.select(&sql, None, None);
            s.analyze();
            snapshot = Some(s);
            Ok(Some(1))
        });

        snapshot.unwrap()
    }

    #[allow(clippy::format_push_string)]
    fn analyze(&mut self) {
        Spi::connect(|client| {
            let parts = self
                .relation_name
                .split('.')
                .map(|name| name.to_string())
                .collect::<Vec<String>>();
            let (schema_name, table_name) = match parts.len() {
                1 => (String::from("public"), parts[0].clone()),
                2 => (parts[0].clone(), parts[1].clone()),
                _ => error!(
                    "Relation name {} is not parsable into schema name and table name",
                    self.relation_name
                ),
            };
            let mut columns: Vec<Column> = Vec::new();
            client.select("SELECT column_name::TEXT, udt_name::TEXT, is_nullable::BOOLEAN, ordinal_position::INTEGER FROM information_schema.columns WHERE table_schema = $1 AND table_name = $2 ORDER BY ordinal_position ASC",
                None,
                Some(vec![
                    (PgBuiltInOids::TEXTOID.oid(), schema_name.into_datum()),
                    (PgBuiltInOids::TEXTOID.oid(), table_name.into_datum()),
                ]))
            .for_each(|row| {
                let name = row[1].value::<String>().unwrap();
                let mut pg_type = row[2].value::<String>().unwrap();
                if pg_type.starts_with('_') {
                    pg_type = pg_type[1..].to_string() + "[]";
                }
                let nullable = row[3].value::<bool>().unwrap();
                let position = row[4].value::<i32>().unwrap() as usize;
                let label = self.y_column_name.contains(&name);
                columns.push(
                    Column {
                        name,
                        pg_type,
                        nullable,
                        label,
                        position,
                        size: 1,
                    }
                );
            });

            for column in &self.y_column_name {
                if !columns.iter().any(|c| c.label && &c.name == column) {
                    error!(
                        "Column `{}` not found. Did you pass the correct `y_column_name`?",
                        column
                    )
                }
            }

            // We have to pull this analysis data into Rust as opposed to using Postgres
            // json_build_object(...), because Postgres functions have a limit of 100 arguments.
            // Any table that has more than 10 columns will exceed the Postgres limit since we
            // calculate 10 statistics per column.
            let mut stats = vec![r#"count(*)::FLOAT4 AS "samples""#.to_string()];
            let mut fields = vec!["samples".to_string()];
            let mut laterals = String::new();
            for column in &columns {
                match column.pg_type.as_str() {
                    "bool" | "int2" | "int4" | "int8" | "float4" | "float8" => {
                        let name = &column.name;
                        let stats_safe_name = column.stats_safe_name();
                        stats.push(format!(r#"min({stats_safe_name})::FLOAT4 AS "{name}_min""#));
                        stats.push(format!(r#"max({stats_safe_name})::FLOAT4 AS "{name}_max""#));
                        stats.push(format!(
                            r#"avg({stats_safe_name})::FLOAT4 AS "{name}_mean""#
                        ));
                        stats.push(format!(
                            r#"stddev({stats_safe_name})::FLOAT4 AS "{name}_stddev""#
                        ));
                        stats.push(format!(r#"percentile_disc(0.25) within group (order by {stats_safe_name})::FLOAT4 AS "{name}_p25""#));
                        stats.push(format!(r#"percentile_disc(0.5) within group (order by {stats_safe_name})::FLOAT4 AS "{name}_p50""#));
                        stats.push(format!(r#"percentile_disc(0.75) within group (order by {stats_safe_name})::FLOAT4 AS "{name}_p75""#));
                        stats.push(format!(
                            r#"count({stats_safe_name})::FLOAT4 AS "{name}_count""#
                        ));
                        stats.push(format!(
                            r#"count(distinct {stats_safe_name})::FLOAT4 AS "{name}_distinct""#
                        ));
                        stats.push(format!(
                            r#"sum(({stats_safe_name} IS NULL)::INT)::FLOAT4 AS "{name}_nulls""#
                        ));
                        fields.push(format!("{name}_min"));
                        fields.push(format!("{name}_max"));
                        fields.push(format!("{name}_mean"));
                        fields.push(format!("{name}_stddev"));
                        fields.push(format!("{name}_p25"));
                        fields.push(format!("{name}_p50"));
                        fields.push(format!("{name}_p75"));
                        fields.push(format!("{name}_count"));
                        fields.push(format!("{name}_distinct"));
                        fields.push(format!("{name}_nulls"));
                    }
                    "bool[]" | "int2[]" | "int4[]" | "int8[]" | "float4[]" | "float8[]" => {
                        let name = &column.name;
                        let stats_safe_name = column.stats_safe_name();
                        let quoted_name = column.quoted_name();
                        let unnested_column = format!(r#""unnested_{}""#, name);
                        let lateral_table = format!(r#""{}_lateral""#, name);
                        stats.push(format!(
                            r#"max(array_ndims({quoted_name}))::FLOAT4 AS "{name}_dims""#
                        ));
                        stats.push(format!(
                            r#"max(cardinality({quoted_name}))::FLOAT4 AS "{name}_cardinality""#
                        ));
                        stats.push(format!(
                            r#"min({lateral_table}.{unnested_column})::FLOAT4 AS "{name}_min""#
                        ));
                        stats.push(format!(
                            r#"max({lateral_table}.{unnested_column})::FLOAT4 AS "{name}_max""#
                        ));
                        fields.push(format!("{name}_dims"));
                        fields.push(format!("{name}_cardinality"));
                        fields.push(format!("{name}_min"));
                        fields.push(format!("{name}_max"));
                        laterals += &format!(", LATERAL (SELECT unnest({stats_safe_name}) AS {unnested_column}) {lateral_table}");
                    }
                    &_ => {
                        error!("unhandled type: `{}` for `{}`", column.pg_type, column.name);
                    }
                }
            }

            let stats = stats.join(", ");
            let sql = format!(
                r#"SELECT {stats} FROM "pgml"."snapshot_{}" {laterals}"#,
                self.id
            );
            let result = client.select(&sql, Some(1), None).first();
            let mut analysis = HashMap::new();
            for (i, field) in fields.iter().enumerate() {
                analysis.insert(
                    field.to_owned(),
                    result
                        .get_datum::<f32>((i + 1).try_into().unwrap())
                        .unwrap(),
                );
            }
            for column in &mut columns {
                let cardinality = format!("{}_cardinality", column.name);
                match analysis.get(&cardinality) {
                    Some(cardinality) => column.size = *cardinality as usize,
                    None => (),
                }
            }
            let analysis_datum = JsonB(json!(analysis));
            let column_datum = JsonB(json!(columns));
            self.analysis = Some(JsonB(json!(analysis)));
            self.columns = Some(JsonB(json!(columns)));
            client.select("UPDATE pgml.snapshots SET status = $1::pgml.status, analysis = $2, columns = $3 WHERE id = $4", Some(1), Some(vec![
                (PgBuiltInOids::TEXTOID.oid(), Status::successful.to_string().into_datum()),
                (PgBuiltInOids::JSONBOID.oid(), analysis_datum.into_datum()),
                (PgBuiltInOids::JSONBOID.oid(), column_datum.into_datum()),
                (PgBuiltInOids::INT8OID.oid(), self.id.into_datum()),
            ]));

            Ok(Some(1))
        });
    }

    pub fn dataset(&self) -> Dataset {
        let mut data = None;
        Spi::connect(|client| {
            let json = self.columns.as_ref().unwrap().0.clone();
            let mut columns: Vec<Column> = serde_json::from_value(json).unwrap();
            columns.sort();
            let sql = format!(
                "SELECT {} FROM {}",
                columns
                    .iter()
                    .map(|c| c.quoted_name())
                    .collect::<Vec<String>>()
                    .join(", "),
                self.snapshot_name()
            );

            let mut num_labels: usize = 0;
            let mut num_features: usize = 0;
            for column in &columns {
                if column.label {
                    num_labels += column.size;
                } else {
                    num_features += column.size;
                }
            }

            // Postgres Arrays arrays are 1 indexed and so are SPI tuples...
            let result = client.select(&sql, None, None);
            let num_rows = result.len();

            let num_test_rows = if self.test_size > 1.0 {
                self.test_size as usize
            } else {
                (num_rows as f32 * self.test_size).round() as usize
            };

            let num_train_rows = num_rows - num_test_rows;
            if num_train_rows == 0 {
                error!(
                    "test_size = {} is too large. There are only {} samples.",
                    num_test_rows, num_rows
                );
            }

            let mut x_train: Vec<f32> = Vec::with_capacity(num_train_rows * num_features);
            let mut y_train: Vec<f32> = Vec::with_capacity(num_train_rows * num_labels);
            let mut x_test: Vec<f32> = Vec::with_capacity(num_test_rows * num_features);
            let mut y_test: Vec<f32> = Vec::with_capacity(num_test_rows * num_labels);

            // result: SpiTupleTable
            // row: SpiHeapTupleData
            // row[i]: SpiHeapTupleDataEntry
            result.enumerate().for_each(|(i, row)| {
                for column in &columns {
                    let vector = if column.label {
                        if i < num_train_rows {
                            &mut y_train
                        } else {
                            &mut y_test
                        }
                    } else if i < num_train_rows {
                        &mut x_train
                    } else {
                        &mut x_test
                    };
                    match column.pg_type.as_str() {
                        "bool" => {
                            vector.push(row[column.position].value::<bool>().unwrap() as u8 as f32)
                        }
                        "bool[]" => {
                            for j in row[column.position].value::<Vec<bool>>().unwrap() {
                                vector.push(j as u8 as f32)
                            }
                        }
                        "int2" => vector.push(row[column.position].value::<i16>().unwrap() as f32),
                        "int2[]" => {
                            for j in row[column.position].value::<Vec<i16>>().unwrap() {
                                vector.push(j as f32)
                            }
                        }
                        "int4" => vector.push(row[column.position].value::<i32>().unwrap() as f32),
                        "int4[]" => {
                            for j in row[column.position].value::<Vec<i32>>().unwrap() {
                                vector.push(j as f32)
                            }
                        }
                        "int8" => vector.push(row[column.position].value::<i64>().unwrap() as f32),
                        "int8[]" => {
                            for j in row[column.position].value::<Vec<i64>>().unwrap() {
                                vector.push(j as f32)
                            }
                        }
                        "float4" => {
                            vector.push(row[column.position].value::<f32>().unwrap() as f32)
                        }
                        "float4[]" => {
                            for j in row[column.position].value::<Vec<f32>>().unwrap() {
                                vector.push(j as f32)
                            }
                        }
                        "float8" => {
                            vector.push(row[column.position].value::<f64>().unwrap() as f32)
                        }
                        "float8[]" => {
                            for j in row[column.position].value::<Vec<f64>>().unwrap() {
                                vector.push(j as f32)
                            }
                        }
                        _ => error!("unhandled type: `{}` for `{}`", column.pg_type, column.name),
                    }
                }
            });

            log!(
                "Snapshot analysis: {}",
                serde_json::to_string(&self.analysis).unwrap()
            );

            let stat = format!("{}_distinct", self.y_column_name[0]);
            let num_distinct_labels = self
                .analysis
                .as_ref()
                .unwrap()
                .0
                .get(stat)
                .unwrap()
                .as_f64()
                .unwrap() as usize;

            data = Some(Dataset {
                x_train,
                y_train,
                x_test,
                y_test,
                num_features,
                num_labels,
                num_rows,
                num_test_rows,
                num_train_rows,
                num_distinct_labels,
            });

            Ok(Some(()))
        });

        let data = data.unwrap();

        info!("{}", data);

        data
    }

    pub fn snapshot_name(&self) -> String {
        format!("\"pgml\".\"snapshot_{}\"", self.id)
    }
}
