import ipaddress
import tomllib
from pathlib import Path
from typing import Literal

from pydantic import BaseModel, ConfigDict, Field, model_validator


class ServerSettings(BaseModel):
    model_config = ConfigDict(extra='forbid')
    host: str
    port: int = Field(gt=0, le=65535)
    api_key_file: Path
    timeout_seconds: int = Field(ge=1, le=3600)
    idle_timeout_seconds: int = Field(ge=1, le=86400)
    maximum_pending_requests: int = Field(ge=1, le=8)
    maximum_request_bytes: int = Field(ge=1024, le=104857600)

    @model_validator(mode='after')
    def validate_host(self) -> 'ServerSettings':
        ipaddress.ip_address(self.host)
        return self


class EngineSettings(BaseModel):
    model_config = ConfigDict(extra='forbid')
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
    model_config = ConfigDict(extra='forbid')
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
    config = Config.model_validate(tomllib.loads(path.read_text()))
    for owner, name in [(config.server, 'api_key_file'), (config.engine, 'path')]:
        value = getattr(owner, name)
        if not value.is_absolute():
            setattr(owner, name, directory / value)
    return config


def read_service_key(config: Config) -> str:
    key = config.server.api_key_file.read_text().strip()
    if not 24 <= len(key) <= 8192:
        raise ValueError('OCR service key must contain 24–8192 characters')
    return key
