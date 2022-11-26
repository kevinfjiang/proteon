FROM rust:latest as build

# create a new empty shell project
RUN USER=root cargo new --bin proteon
WORKDIR /proteon

# copy over your manifests
COPY ./Cargo.lock ./Cargo.lock
COPY ./Cargo.toml ./Cargo.toml

# this build step will cache your dependencies
RUN cargo build --release
RUN rm src/*.rs

# copy your source tree
COPY ./src ./src

# Open port for http connections
EXPOSE 80

# build for release
RUN rm ./target/release/deps/proteon*
RUN cargo build --release

# our final base
FROM rust:latest

# copy the build artifact from the build stage
COPY --from=build /proteon/target/release/proteon .

# set the startup command to run your binary
CMD ["./proteon"]