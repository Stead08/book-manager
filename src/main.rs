use std::{env, net::SocketAddr, sync::Arc};
use chrono::NaiveDateTime;
use serde::{Serialize, Deserialize};
use sqlx::{MySql, MySqlPool, Pool};

use axum::{
    extract::Path,
    http::StatusCode,
    response::IntoResponse,
    routing::{delete, get, patch, post},
    Extension,
    Json, Router};

//書籍を表す構造体
#[derive(Serialize)]
struct Book {
    id: i64,
    title: String,
    author: String,
    publisher: String,
    isbn: String,
    comment: String,
    created_at: NaiveDateTime,
    updated_at: NaiveDateTime,
}

//JSONから値を取り出して保持する構造体
#[derive(Deserialize)]
struct CreateNewBook {
    title: String,
    author: String,
    publisher: String,
    isbn: String,
    comment: String,
}
// JSONから値を取り出して保持する構造体
#[derive(Deserialize)]
struct UpdateComment {
    comment: String,
}

//書籍のリストの情報を表す構造体
#[derive(Serialize)]
struct BookList(Vec<Book>);

type MySqlConPool = Arc<Pool<MySql>>;

// リクエストが送られてくるとHTTPステータスコード”204 No Content"を返すだけのエンドポイント
async fn health_check() -> impl IntoResponse {
    StatusCode::NO_CONTENT
}

async fn book_list(
    Extension(db): Extension<MySqlConPool>,
) -> Result<impl IntoResponse, StatusCode> {
    let conn = db.acquire().await;
    if conn.is_err() {
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    sqlx::query_as!(Book, "select * from books")
        .fetch_all(&mut conn.unwrap())
        .await
        .map(|books| Json(BookList(books)))
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

async fn create_item(
    Json(req): Json<CreateNewBook>,
    Extension(db): Extension<MySqlConPool>,
) -> Result<impl IntoResponse, StatusCode> {
    //コネクションプールからコネクションを取得
    let conn = db.acquire().await;
    if conn.is_err() {
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    // INSERT文実行
    let rows_affected = sqlx::query!(
        r#"
        insert into books (title, author, publisher, isbn, comment, created_at, updated_at) values (?, ?, ?, ?, ?, now(), now())
        "#,
        req.title,
        req.author,
        req.publisher,
        req.isbn,
        req.comment,
    )
    //データベース書き込み処理のためにexecuteを実行
        .execute(&mut conn.unwrap())
        .await
    //クエリが影響した行の数を返すように結果を変形
        .map(|result| result.rows_affected());

    //rows_affectedはResult型なのでパターンマッチで取り出す
    match rows_affected {
        Ok(count) => {
            //影響があった行が一行であれば成功、そうでなければ失敗
            if count == 1 {
                Ok(StatusCode::CREATED)
            } else {
                Err(StatusCode::BAD_REQUEST)
            }
        }
        Err(err) => {
            eprintln!("{:?}", err);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

// 書籍ID（id)と渡されるJSON（req）とデータベースへのコネクションプールを保持
async fn update_comment(
    Path(id): Path<i64>,
    Json(req): Json<UpdateComment>,
    Extension(db): Extension<MySqlConPool>,
) -> Result<impl IntoResponse, StatusCode> {
    let conn = db.acquire().await;
    if conn.is_err() {
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    //UPDATE文を発行
    let rows_affected = sqlx::query!(
        r#"update books set comment = ?, updated_at = now() where id = ?"#,
        req.comment,
        id
    )
        .execute(&mut conn.unwrap())
        .await
        .map(|result| result.rows_affected());

    match rows_affected {
        Ok(count) => {
            if count == 1 {
                Ok(StatusCode::NO_CONTENT)
            } else {
                Err(StatusCode::BAD_REQUEST)
            }
        }
        Err(err) => {
            eprintln!("{:?}", err);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

async fn delete_item(
    Path(id): Path<i64>,
    Extension(db): Extension<MySqlConPool>,
) -> Result<impl IntoResponse, StatusCode> {
    let conn = db.acquire().await;
    if conn.is_err() {
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    let rows_affected = sqlx::query!("delete from books where id = ?", id)
        .execute(&mut conn.unwrap())
        .await
        .map(|result| result.rows_affected());

    match rows_affected {
        Ok(count) => {
            if count == 1 {
                Ok(StatusCode::NO_CONTENT)
            } else {
                Err(StatusCode::BAD_REQUEST)
            }
        }
        Err(err) => {
            eprintln!("{:?}", err);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}
#[tokio::main]
async fn main() -> std::io::Result<()> {
    // sqlxクレートを使ってコネクションプールを用意
    let pool = MySqlPool::connect(&env::var("DATABASE_URL").unwrap())
        .await
        .unwrap();
    //子ルータ
    let books_router = Router::new()
        .route("/", get(book_list))
        .route("/", post(create_item))
        .route("/:id", patch(update_comment))
        .route("/:id", delete(delete_item));
    // 新しいAPIをルータに登録し、コネクションプールを設定
    let app = Router::new()
        .route("/health", get(health_check))
        //親ルータに子ルーターを登録,nestは特定のパスをグルーピングする。
        .nest("/books", books_router)
        .layer(Extension(Arc::new(pool)));
    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();

    Ok(())
}