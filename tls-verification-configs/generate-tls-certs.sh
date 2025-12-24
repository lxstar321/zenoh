#!/bin/bash

# Zenoh 分布式网关 TLS 证书生成脚本
# 生成 CA、服务器证书和客户端证书

set -e

CERTS_DIR="./tls-certs"
CA_KEY="${CERTS_DIR}/ca-key.pem"
CA_CERT="${CERTS_DIR}/ca-cert.pem"

# 网关Router证书
ROUTER_KEY="${CERTS_DIR}/router-key.pem"
ROUTER_CERT="${CERTS_DIR}/router-cert.pem"
ROUTER_CSR="${CERTS_DIR}/router-csr.pem"

# Agent证书
AGENT_KEY="${CERTS_DIR}/agent-key.pem"
AGENT_CERT="${CERTS_DIR}/agent-cert.pem"
AGENT_CSR="${CERTS_DIR}/agent-csr.pem"

# Collector证书
COLLECTOR_KEY="${CERTS_DIR}/collector-key.pem"
COLLECTOR_CERT="${CERTS_DIR}/collector-cert.pem"
COLLECTOR_CSR="${CERTS_DIR}/collector-csr.pem"

# 设备证书
DEVICE_KEY="${CERTS_DIR}/device-key.pem"
DEVICE_CERT="${CERTS_DIR}/device-cert.pem"
DEVICE_CSR="${CERTS_DIR}/device-csr.pem"

# 租户B证书
TENANT_B_AGENT_KEY="${CERTS_DIR}/tenant-b-agent-key.pem"
TENANT_B_AGENT_CERT="${CERTS_DIR}/tenant-b-agent-cert.pem"
TENANT_B_AGENT_CSR="${CERTS_DIR}/tenant-b-agent-csr.pem"

TENANT_B_DEVICE_KEY="${CERTS_DIR}/tenant-b-device-key.pem"
TENANT_B_DEVICE_CERT="${CERTS_DIR}/tenant-b-device-cert.pem"
TENANT_B_DEVICE_CSR="${CERTS_DIR}/tenant-b-device-csr.pem"

log_info() {
    echo "[INFO] $1"
}

log_success() {
    echo "[SUCCESS] $1"
}

log_error() {
    echo "[ERROR] $1"
}

# 创建证书目录
create_cert_dir() {
    log_info "创建证书目录..."
    mkdir -p "$CERTS_DIR"
    log_success "证书目录创建完成: $CERTS_DIR"
}

# 生成CA证书
generate_ca() {
    log_info "生成CA私钥和证书..."
    openssl genrsa -out "$CA_KEY" 2048
    openssl req -new -x509 -key "$CA_KEY" -out "$CA_CERT" -days 365 -subj "/CN=Zenoh Gateway CA/O=Zenoh/CN=CA"
    log_success "CA证书生成完成"
}

# 生成服务器证书 (用于Router)
generate_router_cert() {
    log_info "生成Router服务器证书..."
    openssl genrsa -out "$ROUTER_KEY" 2048
    openssl req -new -key "$ROUTER_KEY" -out "$ROUTER_CSR" -subj "/CN=gateway-router/O=Zenoh/CN=Router"

    openssl x509 -req -in "$ROUTER_CSR" -CA "$CA_CERT" -CAkey "$CA_KEY" -CAcreateserial -out "$ROUTER_CERT" -days 365 -extensions v3_req -extfile <(echo "
[ v3_req ]
basicConstraints = CA:FALSE
keyUsage = nonRepudiation, digitalSignature, keyEncipherment
extendedKeyUsage = serverAuth, clientAuth
subjectAltName = @alt_names

[ alt_names ]
DNS.1 = gateway-router-1
DNS.2 = gateway-router-2
DNS.3 = gateway-router-3
IP.1 = 172.20.0.10
IP.2 = 172.20.0.11
IP.3 = 172.20.0.12
")

    log_success "Router证书生成完成"
}

# 生成Agent证书
generate_agent_cert() {
    log_info "生成Agent客户端证书..."
    openssl genrsa -out "$AGENT_KEY" 2048
    openssl req -new -key "$AGENT_KEY" -out "$AGENT_CSR" -subj "/CN=tenant-a-agent/O=Zenoh/CN=Agent"

    openssl x509 -req -in "$AGENT_CSR" -CA "$CA_CERT" -CAkey "$CA_KEY" -CAcreateserial -out "$AGENT_CERT" -days 365 -extensions v3_req -extfile <(echo "
[ v3_req ]
basicConstraints = CA:FALSE
keyUsage = nonRepudiation, digitalSignature, keyEncipherment
extendedKeyUsage = clientAuth, serverAuth
subjectAltName = @alt_names

[ alt_names ]
DNS.1 = tenant-a-agent
IP.1 = 172.20.0.20
")

    log_success "Agent证书生成完成"
}

# 生成Collector证书
generate_collector_cert() {
    log_info "生成Collector客户端证书..."
    openssl genrsa -out "$COLLECTOR_KEY" 2048
    openssl req -new -key "$COLLECTOR_KEY" -out "$COLLECTOR_CSR" -subj "/CN=data-collector/O=Zenoh/CN=Collector"

    openssl x509 -req -in "$COLLECTOR_CSR" -CA "$CA_CERT" -CAkey "$CA_KEY" -CAcreateserial -out "$COLLECTOR_CERT" -days 365 -extensions v3_req -extfile <(echo "
[ v3_req ]
basicConstraints = CA:FALSE
keyUsage = nonRepudiation, digitalSignature, keyEncipherment
extendedKeyUsage = clientAuth, serverAuth
subjectAltName = @alt_names

[ alt_names ]
DNS.1 = data-collector
IP.1 = 172.20.0.21
")

    log_success "Collector证书生成完成"
}

# 生成设备证书
generate_device_cert() {
    log_info "生成设备客户端证书..."
    openssl genrsa -out "$DEVICE_KEY" 2048
    openssl req -new -key "$DEVICE_KEY" -out "$DEVICE_CSR" -subj "/CN=tenant-a-device/O=Zenoh/CN=Device"

    openssl x509 -req -in "$DEVICE_CSR" -CA "$CA_CERT" -CAkey "$CA_KEY" -CAcreateserial -out "$DEVICE_CERT" -days 365 -extensions v3_req -extfile <(echo "
[ v3_req ]
basicConstraints = CA:FALSE
keyUsage = nonRepudiation, digitalSignature, keyEncipherment
extendedKeyUsage = clientAuth
subjectAltName = @alt_names

[ alt_names ]
DNS.1 = tenant-a-device
IP.1 = 172.20.0.100
")

    log_success "设备证书生成完成"
}

# 生成租户B Agent证书
generate_tenant_b_agent_cert() {
    log_info "生成租户B Agent客户端证书..."
    openssl genrsa -out "$TENANT_B_AGENT_KEY" 2048
    openssl req -new -key "$TENANT_B_AGENT_KEY" -out "$TENANT_B_AGENT_CSR" -subj "/CN=tenant-b-agent/O=Zenoh/CN=Agent"

    openssl x509 -req -in "$TENANT_B_AGENT_CSR" -CA "$CA_CERT" -CAkey "$CA_KEY" -CAcreateserial -out "$TENANT_B_AGENT_CERT" -days 365 -extensions v3_req -extfile <(echo "
[ v3_req ]
basicConstraints = CA:FALSE
keyUsage = nonRepudiation, digitalSignature, keyEncipherment
extendedKeyUsage = clientAuth, serverAuth
subjectAltName = @alt_names

[ alt_names ]
DNS.1 = tenant-b-agent
IP.1 = 172.20.0.30
")

    log_success "租户B Agent证书生成完成"
}

# 生成租户B设备证书
generate_tenant_b_device_cert() {
    log_info "生成租户B设备客户端证书..."
    openssl genrsa -out "$TENANT_B_DEVICE_KEY" 2048
    openssl req -new -key "$TENANT_B_DEVICE_KEY" -out "$TENANT_B_DEVICE_CSR" -subj "/CN=tenant-b-device/O=Zenoh/CN=Device"

    openssl x509 -req -in "$TENANT_B_DEVICE_CSR" -CA "$CA_CERT" -CAkey "$CA_KEY" -CAcreateserial -out "$TENANT_B_DEVICE_CERT" -days 365 -extensions v3_req -extfile <(echo "
[ v3_req ]
basicConstraints = CA:FALSE
keyUsage = nonRepudiation, digitalSignature, keyEncipherment
extendedKeyUsage = clientAuth
subjectAltName = @alt_names

[ alt_names ]
DNS.1 = tenant-b-device
IP.1 = 172.20.0.101
")

    log_success "租户B设备证书生成完成"
}

# 验证证书
verify_certs() {
    log_info "验证证书..."

    # 验证CA证书
    if openssl x509 -in "$CA_CERT" -text -noout > /dev/null 2>&1; then
        log_success "CA证书验证通过"
    else
        log_error "CA证书验证失败"
        return 1
    fi

    # 验证Router证书
    if openssl verify -CAfile "$CA_CERT" "$ROUTER_CERT" > /dev/null 2>&1; then
        log_success "Router证书验证通过"
    else
        log_error "Router证书验证失败"
        return 1
    fi

    # 验证Agent证书
    if openssl verify -CAfile "$CA_CERT" "$AGENT_CERT" > /dev/null 2>&1; then
        log_success "Agent证书验证通过"
    else
        log_error "Agent证书验证失败"
        return 1
    fi

    # 验证Collector证书
    if openssl verify -CAfile "$CA_CERT" "$COLLECTOR_CERT" > /dev/null 2>&1; then
        log_success "Collector证书验证通过"
    else
        log_error "Collector证书验证失败"
        return 1
    fi

    # 验证设备证书
    if openssl verify -CAfile "$CA_CERT" "$DEVICE_CERT" > /dev/null 2>&1; then
        log_success "设备证书验证通过"
    else
        log_error "设备证书验证失败"
        return 1
    fi

    # 验证租户B Agent证书
    if openssl verify -CAfile "$CA_CERT" "$TENANT_B_AGENT_CERT" > /dev/null 2>&1; then
        log_success "租户B Agent证书验证通过"
    else
        log_error "租户B Agent证书验证失败"
        return 1
    fi

    # 验证租户B设备证书
    if openssl verify -CAfile "$CA_CERT" "$TENANT_B_DEVICE_CERT" > /dev/null 2>&1; then
        log_success "租户B设备证书验证通过"
    else
        log_error "租户B设备证书验证失败"
        return 1
    fi

    return 0
}

# 显示证书信息
show_cert_info() {
    echo
    log_info "=== 证书信息 ==="
    echo "CA证书: $CA_CERT"
    echo "Router证书: $ROUTER_CERT"
    echo "Agent证书: $AGENT_CERT"
    echo "Collector证书: $COLLECTOR_CERT"
    echo "设备证书: $DEVICE_CERT"
    echo
    log_info "证书文件列表:"
    ls -la "$CERTS_DIR"/*.pem
    echo
    log_info "证书有效期验证:"
    echo "CA证书:"
    openssl x509 -in "$CA_CERT" -dates -noout
    echo
    echo "Router证书:"
    openssl x509 -in "$ROUTER_CERT" -dates -noout
}

# 清理临时文件
cleanup_temp() {
    log_info "清理临时文件..."
    rm -f "$ROUTER_CSR" "$AGENT_CSR" "$COLLECTOR_CSR" "$DEVICE_CSR" "$TENANT_B_AGENT_CSR" "$TENANT_B_DEVICE_CSR"
    log_success "临时文件清理完成"
}

# 主函数
main() {
    echo "Zenoh 分布式网关 TLS 证书生成工具"
    echo "=================================="
    echo

    create_cert_dir
    generate_ca
    generate_router_cert
    generate_agent_cert
    generate_collector_cert
    generate_device_cert
    generate_tenant_b_agent_cert
    generate_tenant_b_device_cert

    if verify_certs; then
        show_cert_info
        cleanup_temp
        log_success "🎉 所有TLS证书生成完成！"
        echo
        log_info "使用说明:"
        echo "1. 证书已保存在: $CERTS_DIR"
        echo "2. 在docker-compose中挂载证书目录"
        echo "3. 配置各组件使用TLS连接"
        echo "4. 运行: ./quick-verify-tls.sh start"
    else
        log_error "证书生成失败，请检查openssl是否正确安装"
        exit 1
    fi
}

# 执行主函数
main "$@"
