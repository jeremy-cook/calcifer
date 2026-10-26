//! The Calcifer API: the messages and the gRPC service traits generated from
//! `proto/calcifer/v1` at build time. `v1` mirrors the proto package `calcifer.v1`.

pub mod v1 {
    tonic::include_proto!("calcifer.v1");
}
