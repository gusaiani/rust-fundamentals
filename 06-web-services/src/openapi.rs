//! A hand-rolled OpenAPI 3.0 document, served at `/openapi.json` (Pill 14).
//!
//! This file is **given** — like the CLI in Module 5, it's a worked example,
//! not the exercise. It shows that OpenAPI is *just a JSON document with an
//! agreed schema*: `info`, `paths`, `components`. Deriving it from the handler
//! types with `utoipa` (so it can't drift from the code) is a stretch goal.

use axum::Json;
use serde_json::{json, Value};

/// `GET /openapi.json` handler.
pub async fn openapi() -> Json<Value> {
    Json(spec())
}

/// Build the OpenAPI document. Kept deliberately small but valid — paste the
/// output into <https://editor.swagger.io> to render it.
fn spec() -> Value {
    json!({
        "openapi": "3.0.3",
        "info": {
            "title": "taskline",
            "version": "0.1.0",
            "description": "A production-grade task API: register, log in for a JWT, manage your own tasks."
        },
        "components": {
            "securitySchemes": {
                "bearerAuth": { "type": "http", "scheme": "bearer", "bearerFormat": "JWT" }
            },
            "schemas": {
                "RegisterRequest": {
                    "type": "object",
                    "required": ["email", "password"],
                    "properties": {
                        "email": { "type": "string", "format": "email" },
                        "password": { "type": "string", "minLength": 8 }
                    }
                },
                "TokenResponse": {
                    "type": "object",
                    "properties": {
                        "token": { "type": "string" },
                        "token_type": { "type": "string", "example": "Bearer" }
                    }
                },
                "Task": {
                    "type": "object",
                    "properties": {
                        "id": { "type": "string", "format": "uuid" },
                        "title": { "type": "string" },
                        "done": { "type": "boolean" },
                        "created_at": { "type": "string", "format": "date-time" }
                    }
                },
                "CreateTask": {
                    "type": "object",
                    "required": ["title"],
                    "properties": { "title": { "type": "string", "maxLength": 200 } }
                }
            }
        },
        "paths": {
            "/auth/register": {
                "post": {
                    "summary": "Register a new user",
                    "requestBody": { "required": true, "content": { "application/json": {
                        "schema": { "$ref": "#/components/schemas/RegisterRequest" } } } },
                    "responses": {
                        "201": { "description": "Created" },
                        "409": { "description": "Email already registered" },
                        "422": { "description": "Validation failed" }
                    }
                }
            },
            "/auth/login": {
                "post": {
                    "summary": "Log in and receive a JWT",
                    "requestBody": { "required": true, "content": { "application/json": {
                        "schema": { "$ref": "#/components/schemas/RegisterRequest" } } } },
                    "responses": {
                        "200": { "description": "OK", "content": { "application/json": {
                            "schema": { "$ref": "#/components/schemas/TokenResponse" } } } },
                        "401": { "description": "Invalid credentials" }
                    }
                }
            },
            "/tasks": {
                "get": {
                    "summary": "List your tasks",
                    "security": [{ "bearerAuth": [] }],
                    "responses": { "200": { "description": "OK", "content": { "application/json": {
                        "schema": { "type": "array", "items": { "$ref": "#/components/schemas/Task" } } } } } }
                },
                "post": {
                    "summary": "Create a task",
                    "security": [{ "bearerAuth": [] }],
                    "requestBody": { "required": true, "content": { "application/json": {
                        "schema": { "$ref": "#/components/schemas/CreateTask" } } } },
                    "responses": { "201": { "description": "Created", "content": { "application/json": {
                        "schema": { "$ref": "#/components/schemas/Task" } } } } }
                }
            },
            "/tasks/{id}": {
                "get": {
                    "summary": "Get one of your tasks",
                    "security": [{ "bearerAuth": [] }],
                    "parameters": [{ "name": "id", "in": "path", "required": true,
                        "schema": { "type": "string", "format": "uuid" } }],
                    "responses": { "200": { "description": "OK" }, "404": { "description": "Not found" } }
                },
                "patch": {
                    "summary": "Update one of your tasks",
                    "security": [{ "bearerAuth": [] }],
                    "parameters": [{ "name": "id", "in": "path", "required": true,
                        "schema": { "type": "string", "format": "uuid" } }],
                    "responses": { "200": { "description": "OK" }, "404": { "description": "Not found" } }
                },
                "delete": {
                    "summary": "Delete one of your tasks",
                    "security": [{ "bearerAuth": [] }],
                    "parameters": [{ "name": "id", "in": "path", "required": true,
                        "schema": { "type": "string", "format": "uuid" } }],
                    "responses": { "204": { "description": "Deleted" }, "404": { "description": "Not found" } }
                }
            }
        }
    })
}
