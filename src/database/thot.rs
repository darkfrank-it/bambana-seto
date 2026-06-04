use std::result::Result;
use sqlx::{sqlite::SqliteQueryResult, Sqlite, SqlitePool, migrate::MigrateDatabase};
// use tokio::fs;

pub async fn create_db(db_url: &str) -> Result<(), sqlx::Error> {
    if !Sqlite::database_exists(&db_url).await.unwrap_or(false){
        Sqlite::create_database(&db_url).await.unwrap();
        match create_schema(&db_url).await {
            Ok(_) => (), // println!("database created succesfully"),
            Err(e) => panic!("{}", e)
        }
    }
    let pool = SqlitePool::connect(&db_url).await.unwrap();
    let qry = "INSERT INTO settings (description) VALUES($1)";
    let _result = sqlx::query(&qry).bind("testing").execute(&pool).await;
    pool.close().await;
    // println!("{:?}", result);
    Ok(())
}

pub async fn create_schema(db_url:&str) -> Result<SqliteQueryResult, sqlx::Error> {
    let pool = SqlitePool::connect(&db_url).await?;
   
    // let sql_file_path = "./database/panoptes_v0.1.0.sql";
    
    // Include il contenuto del file SQL nel binario
    let qry = include_str!("../sql/panoptes_v0.1.0.sql");


    // Leggi il contenuto del file SQL
    // let qry = fs::read_to_string(sql_file_path).await
    //     .map_err(|e| sqlx::Error::Io(e))?;

    let result = sqlx::query(&qry).execute(&pool).await;
    pool.close().await;
    return result;
}