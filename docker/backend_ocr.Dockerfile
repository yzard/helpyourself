# syntax=docker/dockerfile:1
FROM nvidia/cuda:13.1.2-devel-ubuntu24.04@sha256:b9f64abf7226fdb3463ca202bc99878ec847171e6c5f77bd34c8d1403fbf1eca AS build
ARG DEBIAN_FRONTEND=noninteractive
RUN apt-get update && apt-get install --yes --no-install-recommends \
    cmake libavcodec-dev libavformat-dev libavutil-dev libcurl4-openssl-dev libswscale-dev ninja-build pkg-config \
    && rm -rf /var/lib/apt/lists/*
ADD --checksum=sha256:cf7bf3e151e87fd231ff26ee1f6c64dbe7521d9235105050ecb658d71e10ba22 https://codeload.github.com/Neroued/ninfer/tar.gz/9e163eee4b8acec21ab0ac765107b6a3f287b217 /tmp/ninfer.tar.gz
WORKDIR /src
RUN tar -xzf /tmp/ninfer.tar.gz --strip-components=1 -C /src && rm /tmp/ninfer.tar.gz
RUN cmake -S . -B /build -G Ninja -DCMAKE_BUILD_TYPE=Release -DNINFER_BUILD_APPS=ON \
    -DBUILD_TESTING=OFF -DNINFER_BUILD_BENCHMARKS=OFF \
    && cmake --build /build --parallel 8 --target ninfer ninfer-serve

FROM ghcr.io/astral-sh/uv:0.12.20 AS uv
FROM python:3.12-slim AS checks
COPY --from=uv /uv /usr/local/bin/uv
WORKDIR /workspace
COPY src/backend_ocr/pyproject.toml src/backend_ocr/uv.lock src/backend_ocr/
RUN uv sync --locked --all-groups --project src/backend_ocr
COPY src/backend_ocr/ src/backend_ocr/
COPY tests/backend_ocr/ tests/backend_ocr/
RUN src/backend_ocr/.venv/bin/isort --settings-path src/backend_ocr/pyproject.toml --check-only src/backend_ocr tests/backend_ocr \
    && src/backend_ocr/.venv/bin/black --check --config src/backend_ocr/pyproject.toml src/backend_ocr tests/backend_ocr \
    && src/backend_ocr/.venv/bin/python -m unittest discover -s tests/backend_ocr -p '*.py' \
    && uv export --locked --no-dev --no-emit-project --project src/backend_ocr --output-file /requirements.txt \
    && touch /checks-passed

FROM scratch AS model
ADD --chmod=644 --checksum=sha256:74d2c57145e6ff11d1d2faa79594477f9bc903a611af1fb20218189fbbb77d82 https://huggingface.co/neroued/Qwen3.8-27B-nvfp4-NInfer/resolve/f0b43ad436b9fa8142c6ed6647c470a6fe409484/qwen3_8_27b_nvfp4.ninfer /models/qwen3_8_27b_nvfp4.ninfer
ADD --chmod=644 --checksum=sha256:c71d239df91726fc519c6eb72d318ec65820627232b2f796219e87dcf35d0ab4 https://huggingface.co/neroued/Qwen3.8-27B-nvfp4-NInfer/resolve/f0b43ad436b9fa8142c6ed6647c470a6fe409484/LICENSE /models/LICENSE

FROM nvidia/cuda:13.1.2-runtime-ubuntu24.04@sha256:bff001d3257971cc4752e15ac2d354befa70995ded8e141741ade50569fc192e
ARG DEBIAN_FRONTEND=noninteractive
RUN apt-get update && apt-get install --yes --no-install-recommends \
    python3 python3-venv gosu ca-certificates tzdata libavcodec60 libavformat60 libavutil58 libcurl4t64 libswscale7 \
    && rm -rf /var/lib/apt/lists/*
COPY --from=build /build/apps/ninfer-serve /usr/local/bin/ninfer-serve
COPY --from=build /src/LICENSE /app/licenses/NInfer-LICENSE
COPY --from=checks /requirements.txt /checks-passed /app/
RUN python3 -m venv /opt/app && /opt/app/bin/pip install --no-cache-dir --require-hashes -r /app/requirements.txt
COPY --from=model /models/ /models/
RUN chmod 755 /models
WORKDIR /app
COPY src/backend_ocr/ /app/backend_ocr/
COPY docker/entrypoint.sh /app/entrypoint.sh
RUN chmod 755 /app/entrypoint.sh
ENV PATH=/opt/app/bin:$PATH PUID=1000 PGID=1000 UMASK=077 TZ=UTC PYTHONUNBUFFERED=1
EXPOSE 8000
ENTRYPOINT ["/app/entrypoint.sh", "python3", "-m", "backend_ocr.main"]
CMD ["--data-dir", "/data"]
