use actix_cors::Cors;
use actix_multipart::Multipart;
use actix_web::{web, App, HttpResponse, HttpServer};
use chrono::{DateTime, Utc};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;
use uuid::Uuid;

const UPLOAD_DIR: &str = "./uploads";
const MAX_FILE_SIZE: usize = 10 * 1024 * 1024 * 1024; // 10 GB

#[derive(Debug, Clone, Serialize, Deserialize)]
struct FileInfo {
    id: String,
    name: String,
    size: u64,
    mime_type: String,
    uploaded_at: DateTime<Utc>,
}

struct AppState {
    files: Mutex<Vec<FileInfo>>,
}

impl AppState {
    fn new() -> Self {
        let mut files = Vec::new();
        // Load existing files from disk
        if let Ok(entries) = fs::read_dir(UPLOAD_DIR) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() {
                    let filename = path.file_name().unwrap().to_string_lossy().to_string();
                    if filename.starts_with('.') {
                        continue;
                    }
                    let metadata = fs::metadata(&path).unwrap();
                    let mime = mime_guess::from_path(&path)
                        .first_or_octet_stream()
                        .to_string();
                    files.push(FileInfo {
                        id: Uuid::new_v4().to_string(),
                        name: filename,
                        size: metadata.len(),
                        mime_type: mime,
                        uploaded_at: metadata
                            .modified()
                            .map(|t| DateTime::<Utc>::from(t))
                            .unwrap_or_else(|_| Utc::now()),
                    });
                }
            }
        }
        files.sort_by(|a, b| b.uploaded_at.cmp(&a.uploaded_at));
        AppState {
            files: Mutex::new(files),
        }
    }
}

async fn upload_file(
    mut payload: Multipart,
    data: web::Data<AppState>,
) -> HttpResponse {
    let mut uploaded: Vec<FileInfo> = Vec::new();

    while let Some(Ok(mut field)) = payload.next().await {
        let content_disposition = field.content_disposition().cloned();
        let filename = content_disposition
            .as_ref()
            .and_then(|cd| cd.get_filename().map(|f| sanitize_filename(f)))
            .unwrap_or_else(|| format!("upload_{}", Uuid::new_v4()));

        let file_id = Uuid::new_v4().to_string();
        let filepath = PathBuf::from(UPLOAD_DIR).join(&filename);

        // Handle duplicate names
        let final_path = if filepath.exists() {
            let stem = filepath.file_stem().unwrap().to_string_lossy().to_string();
            let ext = filepath
                .extension()
                .map(|e| format!(".{}", e.to_string_lossy()))
                .unwrap_or_default();
            let new_name = format!("{}_{}{}", stem, &file_id[..8], ext);
            PathBuf::from(UPLOAD_DIR).join(&new_name)
        } else {
            filepath
        };

        let final_name = final_path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_string();

        let mut file = match fs::File::create(&final_path) {
            Ok(f) => f,
            Err(e) => {
                return HttpResponse::InternalServerError()
                    .json(serde_json::json!({"error": format!("Failed to create file: {}", e)}));
            }
        };

        let mut total_size: u64 = 0;
        while let Some(Ok(chunk)) = field.next().await {
            total_size += chunk.len() as u64;
            if total_size > MAX_FILE_SIZE as u64 {
                let _ = fs::remove_file(&final_path);
                return HttpResponse::PayloadTooLarge()
                    .json(serde_json::json!({"error": "File too large (max 10 GB)"}));
            }
            if let Err(e) = file.write_all(&chunk) {
                let _ = fs::remove_file(&final_path);
                return HttpResponse::InternalServerError()
                    .json(serde_json::json!({"error": format!("Write error: {}", e)}));
            }
        }

        let mime = mime_guess::from_path(&final_path)
            .first_or_octet_stream()
            .to_string();

        let info = FileInfo {
            id: file_id,
            name: final_name,
            size: total_size,
            mime_type: mime,
            uploaded_at: Utc::now(),
        };

        uploaded.push(info.clone());
        data.files.lock().unwrap().insert(0, info);
    }

    HttpResponse::Ok().json(serde_json::json!({
        "success": true,
        "files": uploaded
    }))
}

async fn list_files(data: web::Data<AppState>) -> HttpResponse {
    let files = data.files.lock().unwrap();
    HttpResponse::Ok().json(&*files)
}

async fn delete_file(
    path: web::Path<String>,
    data: web::Data<AppState>,
) -> HttpResponse {
    let file_id = path.into_inner();
    let mut files = data.files.lock().unwrap();

    if let Some(pos) = files.iter().position(|f| f.id == file_id) {
        let file_info = files.remove(pos);
        let filepath = PathBuf::from(UPLOAD_DIR).join(&file_info.name);
        let _ = fs::remove_file(filepath);
        HttpResponse::Ok().json(serde_json::json!({"success": true}))
    } else {
        HttpResponse::NotFound().json(serde_json::json!({"error": "File not found"}))
    }
}

async fn download_file(path: web::Path<String>) -> HttpResponse {
    let filename = path.into_inner();
    let filepath = PathBuf::from(UPLOAD_DIR).join(&filename);

    if !filepath.exists() {
        return HttpResponse::NotFound().json(serde_json::json!({"error": "File not found"}));
    }

    let mime = mime_guess::from_path(&filepath)
        .first_or_octet_stream()
        .to_string();

    match fs::read(&filepath) {
        Ok(data) => HttpResponse::Ok()
            .insert_header(("Content-Type", mime.as_str()))
            .insert_header((
                "Content-Disposition",
                format!("attachment; filename=\"{}\"", filename),
            ))
            .body(data),
        Err(_) => HttpResponse::InternalServerError()
            .json(serde_json::json!({"error": "Failed to read file"})),
    }
}

async fn index() -> HttpResponse {
    let html = include_str!("../static/index.html");
    HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(html)
}

fn sanitize_filename(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '.' || c == '-' || c == '_' || c == ' ' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    fs::create_dir_all(UPLOAD_DIR)?;

    let data = web::Data::new(AppState::new());

    let bind_addr = std::env::var("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".to_string());

    println!();
    println!("  ⚡ File Sharing Server");
    println!("  Running on http://{}", bind_addr);
    println!();

    HttpServer::new(move || {
        let cors = Cors::permissive();

        App::new()
            .wrap(cors)
            .app_data(data.clone())
            .app_data(web::PayloadConfig::new(MAX_FILE_SIZE))
            .route("/", web::get().to(index))
            .route("/api/upload", web::post().to(upload_file))
            .route("/api/files", web::get().to(list_files))
            .route("/api/files/{id}", web::delete().to(delete_file))
            .route("/api/download/{filename}", web::get().to(download_file))
    })
    .bind(&bind_addr)?
    .workers(num_cpus())
    .run()
    .await
}

fn num_cpus() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
}
