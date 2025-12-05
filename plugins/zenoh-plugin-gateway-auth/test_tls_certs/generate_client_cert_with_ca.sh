#!/bin/bash
# 生成完整的CA和客户端证书
# 如果CA不存在，会创建新的CA；然后生成客户端证书

set -e

CERT_DIR="/root/luo/project/zenoh/test_tls_certs"
CLIENT_ID=${1:-"sensor-client-001"}  # 从参数获取客户端ID，默认sensor-client-001

CA_KEY="$CERT_DIR/ca_key.pem"
CA_CERT="$CERT_DIR/ca.pem"
CLIENT_KEY="$CERT_DIR/${CLIENT_ID}key.pem"
CLIENT_CSR="$CERT_DIR/${CLIENT_ID}.csr"
CLIENT_CERT="$CERT_DIR/${CLIENT_ID}.pem"

echo "🔐 生成客户端证书: $CLIENT_ID"

# 如果CA不存在，创建新的CA
if [ ! -f "$CA_KEY" ] || [ ! -f "$CA_CERT" ]; then
    echo "📝 创建新的CA证书和密钥..."
    
    # 生成CA私钥
    openssl genrsa -out "$CA_KEY" 2048
    
    # 生成CA自签名证书
    openssl req -new -x509 -key "$CA_KEY" -out "$CA_CERT" -days 3650 \
        -subj "/CN=Test CA/O=Test Organization/C=CN"
    
    echo "✅ CA证书已创建"
fi

# 生成客户端私钥
echo "📝 生成客户端私钥..."
openssl genrsa -out "$CLIENT_KEY" 2048

# 生成证书签名请求，CN设置为客户端ID
echo "📝 生成证书签名请求 (CN=$CLIENT_ID)..."
openssl req -new -key "$CLIENT_KEY" -out "$CLIENT_CSR" \
    -subj "/CN=$CLIENT_ID/O=Test Organization/C=CN"

# 使用CA签名客户端证书（X.509 v3格式）
echo "📝 使用CA签名客户端证书（X.509 v3）..."
openssl x509 -req -in "$CLIENT_CSR" -CA "$CA_CERT" -CAkey "$CA_KEY" \
    -CAcreateserial -out "$CLIENT_CERT" -days 365 -sha256 \
    -extensions v3_req -extfile <(echo "[v3_req]"; echo "keyUsage = digitalSignature, keyEncipherment"; echo "extendedKeyUsage = clientAuth")

# 删除临时文件
rm -f "$CLIENT_CSR" "$CERT_DIR/ca.srl" 2>/dev/null || true

# 验证证书
echo "✅ 验证证书..."
echo "证书CN:"
openssl x509 -in "$CLIENT_CERT" -noout -subject

echo ""
echo "✅ 证书生成完成:"
echo "   CA证书: $CA_CERT"
echo "   CA密钥: $CA_KEY"
echo "   客户端私钥: $CLIENT_KEY"
echo "   客户端证书: $CLIENT_CERT"
echo "   客户端ID (CN): $CLIENT_ID"

