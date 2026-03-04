use crate::{heartbeat, types::DataBatches};
use diesel::{Connection, RunQueryDsl, SqliteConnection, connection::SimpleConnection};
use diesel_migrations::{EmbeddedMigrations, MigrationHarness, embed_migrations};
use futures::StreamExt as _;
use tracing::info;

pub mod models;
pub mod schema;

const MIGRATIONS: EmbeddedMigrations = embed_migrations!("migrations/");

async fn create_db() -> SqliteConnection {
    let now = heartbeat::unix_time();
    let db_file = format!("db/{}.db", now.as_secs_f64());

    let mut connection = SqliteConnection::establish(&db_file).expect(
        "Creating database must succeed, because it is an integral part of the application",
    );
    info!("Connected to database at {db_file}");

    connection
        .run_pending_migrations(MIGRATIONS)
        .expect("Creating schema must succeed, because it is an integral part of the application");

    // With EXTRA, we get additional durability on power loss
    connection
        .batch_execute("PRAGMA synchronous = EXTRA")
        .expect("Having more durable transactions must succeed");

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
            diesel::insert_into(schema::bno055_readings::table)
                .values(&batch.bno_readings)
                .execute(&mut connection)
                .unwrap();
        }

        if !batch.tel0157_readings.is_empty() {
            diesel::insert_into(schema::tel0157_readings::table)
                .values(&batch.tel0157_readings)
                .execute(&mut connection)
                .unwrap();
        }

        if !batch.bmp280_readings.is_empty() {
            diesel::insert_into(schema::bmp280_readings::table)
                .values(&batch.bmp280_readings)
                .execute(&mut connection)
                .unwrap();
        }

        if !batch.as7341_readings.is_empty() {
            diesel::insert_into(schema::as7341_readings::table)
                .values(&batch.as7341_readings)
                .execute(&mut connection)
                .unwrap();
        }

        info!("Processed data batch");
    }
}
