#!/bin/bash
# 生成X.509 v3格式的客户端证书（兼容Zenoh）

set -e

CERT_DIR="/root/luo/project/zenoh/test_tls_certs"
CLIENT_ID=${1:-"sensor-client-001"}

CA_KEY="$CERT_DIR/ca_key.pem"
CA_CERT="$CERT_DIR/ca.pem"
CLIENT_KEY="$CERT_DIR/${CLIENT_ID}key.pem"
CLIENT_CSR="$CERT_DIR/${CLIENT_ID}.csr"
CLIENT_CERT="$CERT_DIR/${CLIENT_ID}.pem"

echo "🔐 生成X.509 v3格式客户端证书: $CLIENT_ID"

# 创建临时配置文件
CONF_FILE=$(mktemp)
cat > "$CONF_FILE" <<EOF
[req]
distinguished_name = req_distinguished_name
req_extensions = v3_req

[req_distinguished_name]

[v3_req]
keyUsage = digitalSignature, keyEncipherment
extendedKeyUsage = clientAuth
EOF

# 生成客户端私钥
echo "📝 生成客户端私钥..."
openssl genrsa -out "$CLIENT_KEY" 2048

# 生成证书签名请求
echo "📝 生成证书签名请求 (CN=$CLIENT_ID)..."
openssl req -new -key "$CLIENT_KEY" -out "$CLIENT_CSR" \
    -subj "/CN=$CLIENT_ID/O=Test Organization/C=CN" \
    -config "$CONF_FILE" -extensions v3_req

# 使用CA签名客户端证书（v3格式）
echo "📝 使用CA签名客户端证书（X.509 v3）..."
openssl x509 -req -in "$CLIENT_CSR" -CA "$CA_CERT" -CAkey "$CA_KEY" \
    -CAcreateserial -out "$CLIENT_CERT" -days 365 -sha256 \
    -extensions v3_req -extfile "$CONF_FILE"

# 删除临时文件
rm -f "$CLIENT_CSR" "$CONF_FILE" "$CERT_DIR/ca.srl" 2>/dev/null || true

# 验证证书版本
echo "✅ 验证证书版本..."
VERSION=$(openssl x509 -in "$CLIENT_CERT" -text -noout | grep "Version" | awk '{print $2}')
echo "   证书版本: $VERSION"

if [ "$VERSION" = "3" ] || [ "$VERSION" = "(0x2)" ]; then
    echo "✅ 证书版本正确（X.509 v3）"
else
    echo "⚠️  证书版本可能不正确，Zenoh需要X.509 v3"
fi

echo ""
echo "✅ 证书生成完成:"
echo "   客户端私钥: $CLIENT_KEY"
echo "   客户端证书: $CLIENT_CERT"
echo "   客户端ID (CN): $CLIENT_ID"


