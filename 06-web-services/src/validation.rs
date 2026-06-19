//! Hand-rolled request validation (Pill 11).
//!
//! The [`Validate`] trait and [`ValidationErrors`] are **given**; implementing
//! `validate` for the request DTOs is **step 2**.
//!
//! We hand-roll instead of pulling the `validator` crate — same ethos as
//! Modules 4 & 5 hand-rolling their arg parsing. A handler calls `body.validate()?`
//! as its first line, so invalid input dies at the boundary (422) before any
//! query runs, and every layer below deals only in already-valid data.

use serde::Serialize;

use crate::models::{CreateTask, RegisterRequest, UpdateTask};

/// A collected set of field errors. Serializes as a list of `{field, message}`
/// so it can be embedded directly in a 422 response body.
#[derive(Debug, Default, Serialize)]
pub struct ValidationErrors {
    pub errors: Vec<FieldError>,
}

#[derive(Debug, Serialize)]
pub struct FieldError {
    pub field: String,
    pub message: String,
}

impl ValidationErrors {
    pub fn new() -> Self {
        ValidationErrors::default()
    }

    /// Record one failure.
    pub fn add(&mut self, field: &str, message: &str) {
        self.errors.push(FieldError {
            field: field.to_string(),
            message: message.to_string(),
        });
    }

    pub fn is_empty(&self) -> bool {
        self.errors.is_empty()
    }

    /// Turn the accumulator into a `Result`: `Ok(())` if clean, else `Err(self)`.
    /// This is the idiomatic tail of a `validate` impl.
    pub fn into_result(self) -> Result<(), ValidationErrors> {
        if self.is_empty() {
            Ok(())
        } else {
            Err(self)
        }
    }
}

/// Anything a handler should check before acting on it.
pub trait Validate {
    fn validate(&self) -> Result<(), ValidationErrors>;
}

impl Validate for RegisterRequest {
    fn validate(&self) -> Result<(), ValidationErrors> {
        let mut errors = ValidationErrors::new();

        if !self.email.contains('@') {
            errors.add("email", "must contain '@'");
        }
        if self.password.len() < 8 {
            errors.add("password", "must be at least 8 characters");
        }

        errors.into_result()
    }
}

impl Validate for CreateTask {
    /// TODO (step 2): title must be non-empty (after trim) and <= 200 chars.
    fn validate(&self) -> Result<(), ValidationErrors> {
        let mut errors = ValidationErrors::new();

        let trimmed = self.title.trim();
        if trimmed.is_empty() {
            errors.add("title", "must not be empty");
        }
        if trimmed.chars().count() > 200 {
            errors.add("title", "must be at most 200 characters");
        }

        errors.into_result()
    }
}

impl Validate for UpdateTask {
    /// TODO (step 2): if `title` is `Some`, apply the same rule as `CreateTask`;
    /// `None` fields are simply not updated, so they need no checking.
    fn validate(&self) -> Result<(), ValidationErrors> {
        let mut errors = ValidationErrors::new();

        if let Some(title) = &self.title {
            let trimmed = title.trim();
            if trimmed.is_empty() {
                errors.add("title", "must not be empty");
            }
            if trimmed.chars().count() > 200 {
                errors.add("title", "must be at most 200 characters");
            }
        }

        errors.into_result()
    }
}
