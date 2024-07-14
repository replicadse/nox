#!/bin/bash

openssl genpkey -algorithm RSA -out ./ca/ca.key
openssl req -x509 -new -key ./ca/ca.key -out ./ca/ca.crt -subj "/C=US"

openssl genpkey -algorithm RSA -out ./server/server.key
openssl req -new -key ./server/server.key -out ./server/server.csr -config server.openssl.cnf -subj "/C=US"
openssl x509 -req -in ./server/server.csr -CA ./ca/ca.crt -CAkey ./ca/ca.key -CAcreateserial -out ./server/server.crt -extensions req_ext -extfile server.openssl.cnf

openssl genpkey -algorithm RSA -out ./client/cert/client.key
openssl req -new -key ./client/cert/client.key -out ./client/cert/client.csr -subj "/C=US"
openssl x509 -req -in ./client/cert/client.csr -CA ./ca/ca.crt -CAkey ./ca/ca.key -CAcreateserial -out ./client/cert/client.crt

# then connect with
# wscat --cert ./cert/client/cert/client.crt --key ./cert/client/cert/client.key --ca ./cert/ca/ca.crt -c wss://localhost:8080/ws
