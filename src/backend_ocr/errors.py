from typing import Any


class OcrError(Exception):
    def __init__(self, status: int, code: str, archive: dict[str, Any]) -> None:
        super().__init__(code)
        self.status = status
        self.code = code
        self.archive = archive
