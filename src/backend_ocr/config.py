import ipaddress
import tomllib
from pathlib import Path
from typing import Literal

from pydantic import BaseModel, ConfigDict, Field, SecretStr, ValidationError, field_validator, model_validator


class ServerSettings(BaseModel):
    model_config = ConfigDict(extra='forbid', hide_input_in_errors=True)
    host: str
    port: int = Field(gt=0, le=65535)
    api_key: SecretStr = Field(min_length=24, max_length=8192)
    timeout_seconds: int = Field(ge=1, le=3600)
    idle_timeout_seconds: int = Field(ge=1, le=86400)
    maximum_pending_requests: int = Field(ge=1, le=8)
    maximum_request_bytes: int = Field(ge=1024, le=104857600)

    @field_validator('api_key')
    @classmethod
    def validate_key(cls, value: SecretStr) -> SecretStr:
        if not all(33 <= ord(character) <= 126 for character in value.get_secret_value()):
            raise ValueError('OCR API key must contain visible ASCII characters without whitespace')
        return value

    @model_validator(mode='after')
    def validate_host(self) -> 'ServerSettings':
        ipaddress.ip_address(self.host)
        return self


class EngineSettings(BaseModel):
    model_config = ConfigDict(extra='forbid', hide_input_in_errors=True)
    binary: str = Field(min_length=1)
    path: Path
    model: Literal['qwen3.8-27b-ninfer-nvfp4']
    port: int = Field(gt=0, le=65535)
    context_length: int = Field(ge=8192, le=81920)
    output_tokens: int = Field(ge=256, le=32768)
    image_pixel_budget: int = Field(ge=65536, le=16777216)
    maximum_image_pixels: int = Field(ge=65536, le=100000000)
    temperature: float = Field(ge=0, le=2)
    seed: int = Field(ge=0)


class Config(BaseModel):
    model_config = ConfigDict(extra='forbid', hide_input_in_errors=True)
    server: ServerSettings
    engine: EngineSettings

    @model_validator(mode='after')
    def validate_relationships(self) -> 'Config':
        if self.server.port == self.engine.port or self.engine.output_tokens >= self.engine.context_length:
            raise ValueError('Separate service/engine ports and output below context capacity are required')
        return self


def load_config(data_dir: Path) -> Config:
    if not data_dir.is_absolute():
        raise ValueError('--data-dir must be an absolute directory')
    directory = data_dir.resolve(strict=True)
    if not directory.is_dir():
        raise ValueError('--data-dir must be a directory')
    path = directory / 'config.toml'
    try:
        config = Config.model_validate(tomllib.loads(path.read_text()))
    except (tomllib.TOMLDecodeError, ValidationError):
        raise ValueError('Invalid OCR TOML configuration; check schema and field values') from None
    if not config.engine.path.is_absolute():
        config.engine.path = directory / config.engine.path
    return config
