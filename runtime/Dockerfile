FROM denoland/deno:2.3.3

WORKDIR /app

COPY main.ts .

# Cache imports at build time (none in this file, but keeps the layer clean)
RUN deno cache main.ts || true

EXPOSE 8000

CMD ["deno", "run", \
     "--allow-net", \
     "--allow-env", \
     "--allow-read=/tmp,/opt/shipyard/data", \
     "--allow-write=/tmp", \
     "main.ts"]
