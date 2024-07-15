#!/bin/bash

rm -rf ./gen

mkdir -p ./gen/ca
mkdir -p ./gen/server
mkdir -p ./gen/client/a/cert
mkdir -p ./gen/client/a/gpg
mkdir -p ./gen/client/b/cert
mkdir -p ./gen/client/b/gpg

openssl genpkey -algorithm RSA -out ./gen/ca/ca.key
openssl req -x509 -new -key ./gen/ca/ca.key -out ./gen/ca/ca.crt -subj "/C=US"

openssl genpkey -algorithm RSA -out ./gen/server/server.key
openssl req -new -key ./gen/server/server.key -out ./gen/server/server.csr -config server.openssl.cnf -subj "/C=US"
openssl x509 -req -in ./gen/server/server.csr -CA ./gen/ca/ca.crt -CAkey ./gen/ca/ca.key -CAcreateserial -out ./gen/server/server.crt -extensions req_ext -extfile server.openssl.cnf

openssl genpkey -algorithm RSA -out ./gen/client/a/cert/client.key
openssl req -new -key ./gen/client/a/cert/client.key -out ./gen/client/a/cert/client.csr -subj "/C=US"
openssl x509 -req -in ./gen/client/a/cert/client.csr -CA ./gen/ca/ca.crt -CAkey ./gen/ca/ca.key -CAcreateserial -out ./gen/client/a/cert/client.crt

openssl genpkey -algorithm RSA -out ./gen/client/b/cert/client.key
openssl req -new -key ./gen/client/b/cert/client.key -out ./gen/client/b/cert/client.csr -subj "/C=US"
openssl x509 -req -in ./gen/client/b/cert/client.csr -CA ./gen/ca/ca.crt -CAkey ./gen/ca/ca.key -CAcreateserial -out ./gen/client/b/cert/client.crt

# then connect with
wscat --cert ./cert/gen/client/a/cert/client.crt --key ./cert/gen/client/a/cert/client.key --ca ./cert/gen/ca/ca.crt -c wss://localhost:8080/ws
