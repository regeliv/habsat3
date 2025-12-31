use crate::types::DataBatches;
use diesel::{Connection, RunQueryDsl, SqliteConnection};
use diesel_migrations::{EmbeddedMigrations, MigrationHarness, embed_migrations};
use futures::StreamExt as _;
use tokio::io;
use tracing::info;

pub mod models;
pub mod schema;

const DB_PATH: &str = "habsat.db";
const MIGRATIONS: EmbeddedMigrations = embed_migrations!("migrations/");

async fn create_db() -> SqliteConnection {
    match tokio::fs::remove_file(DB_PATH).await {
        Err(e) if e.kind() != io::ErrorKind::NotFound => {
            panic!("Failed to remove old database file");
        }
        _ => {}
    }

    let mut connection = SqliteConnection::establish(DB_PATH).expect(
        "Creating database must succeed, because it is an integral part of the application",
    );
    info!("Connected to database at {DB_PATH}");

    connection
        .run_pending_migrations(MIGRATIONS)
        .expect("Creating schema must succeed, because it is an integral part of the application");
    info!("Created database schema");

    connection
}

pub async fn db_task(data_batches: kanal::AsyncReceiver<DataBatches>) {
    let mut connection = create_db().await;

    let mut data_batches = data_batches.stream();

    while let Some(batch) = data_batches.next().await {
        if !batch.cpu_temps.is_empty() {
            diesel::insert_into(schema::cpu_temperature::table)
                .values(&batch.cpu_temps)
                .execute(&mut connection)
                .unwrap();
        }

        if !batch.mem_usages.is_empty() {
            diesel::insert_into(schema::memory_usage::table)
                .values(&batch.mem_usages)
                .execute(&mut connection)
                .unwrap();
        }

        if !batch.fs_usages.is_empty() {
            diesel::insert_into(schema::filesystem_usage::table)
                .values(&batch.fs_usages)
                .execute(&mut connection)
                .unwrap();
        }

        if !batch.bno_readings.is_empty() {
            diesel::insert_into(schema::bno_sensor_data::table)
                .values(&batch.bno_readings)
                .execute(&mut connection)
                .unwrap();
        }

        info!("Processed data batch");
    }
}
