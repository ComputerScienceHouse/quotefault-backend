FROM rust:latest as build

WORKDIR /app
COPY . .
ENV SQLX_OFFLINE true
RUN cargo build --release

FROM rust:latest as serve
WORKDIR /app
COPY --from=build /app/target/release/quotefault-backend ./
CMD ["./quotefault-backend"]
