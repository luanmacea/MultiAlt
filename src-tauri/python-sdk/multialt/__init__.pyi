"""Type stubs for the MultiAlt API (app-owned). The reference is docs/API.md."""

from contextlib import AbstractContextManager
from dataclasses import dataclass
from typing import Any, Optional, Sequence, Union

import numpy

from ._core.protocol import (
    BadArgs as BadArgs,
    CaptureFailed as CaptureFailed,
    FocusTimeout as FocusTimeout,
    Forbidden as Forbidden,
    Minimized as Minimized,
    MultiAltError as MultiAltError,
    NoFocusLease as NoFocusLease,
    NotForeground as NotForeground,
    Stopped as Stopped,
    UnsupportedKey as UnsupportedKey,
    UntrackedWindow as UntrackedWindow,
)

RGB = tuple[int, int, int]
Point = tuple[float, float]
Region = tuple[float, float, float, float]

@dataclass(frozen=True)
class Account:
    user_id: int
    name: str
    alias: str = ""
    group: str = ""
    in_game: bool = False
    place_id: int = 0
    job_id: str = ""

    @classmethod
    def from_json(cls, data: dict[str, Any]) -> "Account": ...
    def __str__(self) -> str: ...
    def focused(self, timeout_ms: int = 20000) -> AbstractContextManager[None]: ...
    def press(self, key: str, hold_ms: int = 40) -> None: ...
    def key_down(self, key: str) -> None: ...
    def key_up(self, key: str) -> None: ...
    def type_text(self, text: str, delay_ms: int = 30) -> None: ...
    def click(self, x: float, y: float) -> None: ...
    def pixel(self, x: float, y: float) -> RGB: ...
    def wait_pixel(
        self,
        x: float,
        y: float,
        rgb: RGB,
        tolerance: int = 12,
        timeout: float = 10.0,
        interval: float = 0.1,
    ) -> bool: ...
    def find_image(
        self,
        asset: str,
        threshold: float = 0.85,
        region: Optional[Region] = None,
        scales: Optional[Sequence[float]] = None,
    ) -> Optional[Point]: ...
    def wait_image(
        self,
        asset: str,
        timeout: float = 10.0,
        threshold: float = 0.85,
        region: Optional[Region] = None,
        interval: float = 0.5,
    ) -> Optional[Point]: ...
    def click_image(
        self,
        asset: str,
        timeout: float = 10.0,
        threshold: float = 0.85,
        region: Optional[Region] = None,
    ) -> bool: ...
    def capture(self, region: Optional[Region] = None) -> numpy.ndarray: ...
    def status(self, text: str) -> None: ...
    def log(self, *args: object, level: str = "log") -> None: ...

@dataclass
class RunContext:
    id: str = ""
    script: str = ""
    mode: str = "one_at_a_time"
    trigger: dict[str, Any] = ...
    source: str = "app"
    accounts: list[Account] = ...

def accounts() -> list[Account]: ...
def run_context() -> RunContext: ...
def log(*args: object, level: str = "log") -> None: ...
def status(text: str) -> None: ...
def sleep(seconds: float) -> None: ...
def stopped() -> bool: ...

class Dashboard:
    def define(self, layout: list[dict[str, Any]]) -> None: ...
    def set(self, widget_id: str, value: Any, account: Union[Account, int, None] = None) -> None: ...
    def push(self, widget_id: str, value: Any) -> None: ...
    def clear(self) -> None: ...
    def farm_report(self) -> None: ...
    def report_status(self, account: Account, text: str) -> None: ...
    def report_loop(self, account: Account) -> None: ...
    def report_error(self, account: Account, err: object) -> None: ...

dashboard: Dashboard
