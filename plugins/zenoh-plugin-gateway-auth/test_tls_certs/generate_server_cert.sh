#!/bin/bash
# 使用新CA生成服务器证书

set -e

CERT_DIR="/root/luo/project/zenoh/test_tls_certs"
CA_KEY="$CERT_DIR/ca_key.pem"
CA_CERT="$CERT_DIR/ca.pem"
SERVER_KEY="$CERT_DIR/serversidekey.pem"
SERVER_CSR="$CERT_DIR/serverside.csr"
SERVER_CERT="$CERT_DIR/serverside.pem"

echo "🔐 生成服务器证书（使用新CA）"

# 生成服务器私钥（如果不存在）
if [ ! -f "$SERVER_KEY" ]; then
    echo "📝 生成服务器私钥..."
    openssl genrsa -out "$SERVER_KEY" 2048
fi

# 生成证书签名请求
echo "📝 生成服务器证书签名请求..."
openssl req -new -key "$SERVER_KEY" -out "$SERVER_CSR" \
    -subj "/CN=localhost/O=Test Organization/C=CN"

# 使用CA签名服务器证书
echo "📝 使用CA签名服务器证书..."
openssl x509 -req -in "$SERVER_CSR" -CA "$CA_CERT" -CAkey "$CA_KEY" \
    -CAcreateserial -out "$SERVER_CERT" -days 365 -sha256

# 删除临时文件
rm -f "$SERVER_CSR" "$CERT_DIR/ca.srl" 2>/dev/null || true

echo "✅ 服务器证书已生成"
openssl x509 -in "$SERVER_CERT" -noout -subject


