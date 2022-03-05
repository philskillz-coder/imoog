use crate::options::{MongoOptions, PostgresOptions};
use async_trait::async_trait;
use mongodb::{
    bson::doc, 
    options::ClientOptions,
    Collection,
    Client
};
use sqlx::postgres::PgPoolOptions;
use serde::{Deserialize, Serialize};

#[async_trait]
trait DatabaseImpl<OptionsT> {
    async fn connect(options: OptionsT) -> Self;
    async fn fetch(&self, identifier: String) -> Option<(String, Vec<u8>, String)>;
    async fn insert(&self, identifier: String, mime_type: String, image: Vec<u8>);
    async fn delete(&self, identifier: String);
}

struct DatabaseDriver<OptionsT, ConnectionT> {
    options: OptionsT,
    connection: ConnectionT,
}

#[derive(Debug, Deserialize, Serialize)]
struct MongoImoogCollection {
    _id: String,
    image_data: Vec<u8>,
    mime: String
}

#[async_trait]
impl DatabaseImpl<MongoOptions> for DatabaseDriver<MongoOptions, Collection<MongoImoogCollection>> {
    async fn connect(options: MongoOptions) -> Self {
        let client_options = ClientOptions::parse(&options.connection_uri)
            .await
            .expect("Failed to parse connection uri");
        let client =
            Client::with_options(client_options).expect("Failed to construct client (MongoDB)");

        let db = client.database(&options.database_name);
        let collection = db.collection::<MongoImoogCollection>(&options.collection_name);

        Self {
            options,
            connection: collection,
        }
    }

    async fn fetch(&self, identifier: String) -> Option<(String, Vec<u8>, String)> {
        let filter = doc! {"_id": identifier};
        let document = self.connection.find_one(filter, None)
            .await
            .expect("Query failed");

        document.map(|d| (d._id, d.image_data, d.mime))
    }

    async fn insert(&self, identifier: String, mime_type: String, image: Vec<u8>) {
        let image = MongoImoogCollection {
            _id: identifier,
            mime: mime_type,
            image_data: image
        };

        self.connection.insert_one(image, None)
            .await
            .expect("Failed to insert image (MongoDB)");
    }

    async fn delete(&self, identifier: String) {
        let filter = doc! { "_id": identifier };
        self.connection.delete_one(filter, None)
            .await
            .expect("Failed to delete image (MongoDB)");
    }
}

#[cfg(features = "postgres")]
#[async_trait]
impl DatabaseImpl<PostgresOptions> for DatabaseDriver<PostgresOptions, sqlx::Pool<sqlx::Postgres>> {
    async fn connect(options: PostgresOptions) -> Self {
        let connection_uri = &options.connection_uri;
        let max_connections = &options.max_connections;

        let conn = PgPoolOptions::new()
            .max_connections(max_connections.to_owned())
            .connect(&connection_uri)
            .await
            .expect("Failed to connect to PostgreSQL database");

        let db = Self {
            options,
            connection: conn,
        };

        // execute the basic table initialization for imoog
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS imoog (
            image_identifier TEXT PRIMARY KEY,
            image_data BLOB,
            mime TEXT
        )",
        )
        .execute(&db.connection)
        .await
        .expect("Failed to create imoog PostgreSQL table");

        db
    }

    async fn fetch(&self, identifier: String) -> (String, Vec<u8>, String) {
        /*
        Schema:
        image_identifier TEXT PRIMARY KEY,
        image_data BLOB
        mime TEXT
        */
        let row: (String, Vec<u8>, String) =
            sqlx::query_as("SELECT * FROM imoog WHERE image_identifier = $1")
                .bind(identifier)
                .fetch_one(&self.connection)
                .await
                .unwrap();

        row
    }

    async fn insert(&self, identifier: String, mime_type: String, image: Vec<u8>) {
        sqlx::query("INSERT INTO imoog VALUES($1, $2, $3)")
            .bind(&identifier)
            .bind(image)
            .bind(mime_type)
            .execute(&self.connection)
            .await
            .expect(&format!("Failed to insert image ({})", identifier));
    }

    async fn delete(&self, identifier: String) {
        sqlx::query("DELETE FROM imoog WHERE image_identifier = $1")
            .bind(&identifier)
            .execute(&self.connection)
            .await
            .expect(&format!("Failed to delete image ({})", identifier));
    }
}