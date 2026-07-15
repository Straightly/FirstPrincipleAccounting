"""Errors raised by calls into the runtime backend (Impl Spec §4.4)."""

from __future__ import annotations


class BackendApiError(Exception):
    """A non-2xx response from the runtime backend's HTTP API.

    Mirrors the backend's own error body (`{error_code, message, details?}`,
    Impl Spec §4.4) rather than inventing a separate error vocabulary — a
    caller inspecting `error_code` sees exactly what the backend sent.
    """

    def __init__(
        self,
        status: int,
        error_code: str,
        message: str,
        details: object | None = None,
    ) -> None:
        super().__init__(f"{error_code} ({status}): {message}")
        self.status = status
        self.error_code = error_code
        self.message = message
        self.details = details
