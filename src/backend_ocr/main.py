import argparse
import logging
from pathlib import Path

import uvicorn

from .app import create_application
from .config import load_config


def main() -> None:
    logging.basicConfig(level=logging.INFO, format='%(asctime)s %(levelname)s %(name)s %(message)s')
    parser = argparse.ArgumentParser(description='Private Qwen3.8/NInfer health document service')
    parser.add_argument('--config', type=Path, required=True)
    config = load_config(parser.parse_args().config)
    if not config.engine.path.is_file():
        parser.error('Configured NInfer model artifact is missing')
    uvicorn.run(
        create_application(config),
        host=config.server.host,
        port=config.server.port,
        workers=1,
        access_log=False,
        timeout_graceful_shutdown=15,
    )


if __name__ == '__main__':
    main()
