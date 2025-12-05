#!/bin/bash
# 生成sensor-client-001的客户端证书

set -e

CERT_DIR="/root/luo/project/zenoh/test_tls_certs"
CA_KEY="$CERT_DIR/ca_key.pem"  # 使用CA私钥
CA_CERT="$CERT_DIR/ca.pem"
CLIENT_ID="sensor-client-001"
CLIENT_KEY="$CERT_DIR/${CLIENT_ID}key.pem"
CLIENT_CSR="$CERT_DIR/${CLIENT_ID}.csr"
CLIENT_CERT="$CERT_DIR/${CLIENT_ID}.pem"

echo "🔐 生成客户端证书: $CLIENT_ID"

# 检查CA证书是否存在
if [ ! -f "$CA_CERT" ]; then
    echo "❌ CA证书不存在: $CA_CERT"
    exit 1
fi

# 生成客户端私钥
echo "📝 生成客户端私钥..."
openssl genrsa -out "$CLIENT_KEY" 2048

# 生成证书签名请求，CN设置为客户端ID
echo "📝 生成证书签名请求 (CN=$CLIENT_ID)..."
openssl req -new -key "$CLIENT_KEY" -out "$CLIENT_CSR" -subj "/CN=$CLIENT_ID/O=Test Organization/C=CN"

# 使用CA签名客户端证书
echo "📝 使用CA签名客户端证书..."
openssl x509 -req -in "$CLIENT_CSR" -CA "$CA_CERT" -CAkey "$CA_KEY" -CAcreateserial \
    -out "$CLIENT_CERT" -days 365 -sha256

# 验证证书
echo "✅ 验证证书..."
openssl x509 -in "$CLIENT_CERT" -noout -subject

echo ""
echo "✅ 证书生成完成:"
echo "   私钥: $CLIENT_KEY"
echo "   证书: $CLIENT_CERT"
echo "   CN: $CLIENT_ID"

