#!/bin/bash

# Zenoh mTLS 证书生成脚本
# 此脚本生成用于双向 TLS 认证的证书

set -e

# 配置变量
CERT_DIR="./certs"
CA_KEY="${CERT_DIR}/ca-key.pem"
CA_CERT="${CERT_DIR}/ca-cert.pem"
SERVER_KEY="${CERT_DIR}/server-key.pem"
SERVER_CERT="${CERT_DIR}/server-cert.pem"
CLIENT_KEY="${CERT_DIR}/client-key.pem"
CLIENT_CERT="${CERT_DIR}/client-cert.pem"

# 证书配置
COUNTRY="CN"
STATE="Beijing"
CITY="Beijing"
ORG="Zenoh Example"
CA_CN="Zenoh CA"
SERVER_CN="zenoh-server"
CLIENT_CN="zenoh-client"
CLIENT_OU="sensor"  # 客户端类型：sensor, controller, gateway, monitor

# 创建证书目录
mkdir -p "$CERT_DIR"

echo "生成 Zenoh mTLS 证书..."

# 1. 生成 CA 私钥
echo "1. 生成 CA 私钥..."
openssl genrsa -out "$CA_KEY" 4096

# 2. 生成 CA 自签名证书
echo "2. 生成 CA 证书..."
cat > "${CERT_DIR}/ca.conf" << EOF
[req]
distinguished_name = req_distinguished_name
req_extensions = v3_req
prompt = no

[req_distinguished_name]
C = $COUNTRY
ST = $STATE
L = $CITY
O = $ORG
CN = $CA_CN

[v3_req]
basicConstraints = critical,CA:TRUE
keyUsage = critical,keyCertSign,cRLSign
subjectKeyIdentifier = hash
EOF

openssl req -new -x509 -days 3650 -key "$CA_KEY" -sha256 -out "$CA_CERT" \
  -config "${CERT_DIR}/ca.conf" -extensions v3_req

# 3. 生成服务端私钥
echo "3. 生成服务端私钥..."
openssl genrsa -out "$SERVER_KEY" 2048

# 4. 生成服务端证书签名请求
echo "4. 生成服务端证书请求..."
cat > "${CERT_DIR}/server.conf" << EOF
[req]
distinguished_name = req_distinguished_name
req_extensions = v3_req
prompt = no

[req_distinguished_name]
C = $COUNTRY
ST = $STATE
L = $CITY
O = $ORG
CN = $SERVER_CN

[v3_req]
basicConstraints = CA:FALSE
keyUsage = digitalSignature,keyEncipherment
extendedKeyUsage = serverAuth
subjectAltName = @alt_names

[alt_names]
DNS.1 = localhost
IP.1 = 127.0.0.1
IP.2 = 0.0.0.0
EOF

openssl req -new -key "$SERVER_KEY" -out "${CERT_DIR}/server.csr" \
  -config "${CERT_DIR}/server.conf"

# 5. 使用 CA 签署服务端证书
echo "5. 签署服务端证书..."
openssl x509 -req -days 365 -in "${CERT_DIR}/server.csr" -CA "$CA_CERT" \
  -CAkey "$CA_KEY" -sha256 -out "$SERVER_CERT" \
  -CAcreateserial -CAserial "${CERT_DIR}/ca.srl" \
  -extfile "${CERT_DIR}/server.conf" -extensions v3_req

# 6. 生成客户端私钥
echo "6. 生成客户端私钥..."
openssl genrsa -out "$CLIENT_KEY" 2048

# 7. 生成客户端证书签名请求
echo "7. 生成客户端证书请求..."
cat > "${CERT_DIR}/client.conf" << EOF
[req]
distinguished_name = req_distinguished_name
req_extensions = v3_req
prompt = no

[req_distinguished_name]
C = $COUNTRY
ST = $STATE
L = $CITY
O = $ORG
OU = $CLIENT_OU
CN = $CLIENT_CN

[v3_req]
basicConstraints = CA:FALSE
keyUsage = digitalSignature,keyEncipherment
extendedKeyUsage = clientAuth
subjectAltName = @alt_names

[alt_names]
DNS.1 = localhost
IP.1 = 127.0.0.1
EOF

openssl req -new -key "$CLIENT_KEY" -out "${CERT_DIR}/client.csr" \
  -config "${CERT_DIR}/client.conf"

# 8. 使用 CA 签署客户端证书
echo "8. 签署客户端证书..."
openssl x509 -req -days 365 -in "${CERT_DIR}/client.csr" -CA "$CA_CERT" \
  -CAkey "$CA_KEY" -sha256 -out "$CLIENT_CERT" \
  -CAcreateserial -CAserial "${CERT_DIR}/ca.srl" \
  -extfile "${CERT_DIR}/client.conf" -extensions v3_req

# 9. 设置适当的文件权限
chmod 600 "$CA_KEY" "$SERVER_KEY" "$CLIENT_KEY"
chmod 644 "$CA_CERT" "$SERVER_CERT" "$CLIENT_CERT"

# 清理临时文件
rm -f "${CERT_DIR}/server.csr" "${CERT_DIR}/client.csr" "${CERT_DIR}/ca.srl" \
      "${CERT_DIR}/ca.conf" "${CERT_DIR}/server.conf" "${CERT_DIR}/client.conf"

echo ""
echo "证书生成完成！"
echo ""
echo "生成的证书文件："
echo "  CA 证书: $CA_CERT"
echo "  服务端证书: $SERVER_CERT"
echo "  服务端私钥: $SERVER_KEY"
echo "  客户端证书: $CLIENT_CERT"
echo "  客户端私钥: $CLIENT_KEY"
echo ""
echo "在 Zenoh 配置中设置以下路径："
echo "  root_ca_certificate: \"$CA_CERT\""
echo "  listen_certificate: \"$SERVER_CERT\""
echo "  listen_private_key: \"$SERVER_KEY\""
echo "  connect_certificate: \"$CLIENT_CERT\""
echo "  connect_private_key: \"$CLIENT_KEY\""
echo ""
echo "客户端证书信息："
openssl x509 -in "$CLIENT_CERT" -text -noout | grep -E "Subject:|CN =|OU ="
echo ""
echo "配置文件："
echo "  mTLS配置: mtls_config_example.json5"
echo "  ACL配置: mtls_acl_config.json5"
echo ""
echo "注意："
echo "- 确保证书文件路径在配置文件中正确设置"
echo "- ACL配置已启用，默认拒绝策略，只允许指定话题"
echo "- 在生产环境中，使用更强的加密参数和适当的证书过期时间"
echo "- 定期轮换证书以确保安全性"
