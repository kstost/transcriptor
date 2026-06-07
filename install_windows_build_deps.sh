#!/bin/bash
# Windows 크로스 컴파일에 필요한 시스템 패키지 설치

set -e

if [ "$(id -u)" -ne 0 ]; then
    echo "root 권한이 필요합니다. sudo로 실행해주세요."
    exit 1
fi

# MSVC STL bundled by cargo-xwin currently requires clang-cl 19+.
LLVM_VERSION="19"
if ! apt-cache show "clang-${LLVM_VERSION}" >/dev/null 2>&1; then
    echo "clang-${LLVM_VERSION} 패키지를 찾을 수 없습니다."
    exit 1
fi

echo "탐지된 LLVM 버전: ${LLVM_VERSION}"

apt update
apt install -y "clang-${LLVM_VERSION}" "clang-tools-${LLVM_VERSION}" "lld-${LLVM_VERSION}" "llvm-${LLVM_VERSION}"

# 버전 없는 심볼릭 링크 생성 (cargo-xwin이 필요로 함). /usr/local/bin이
# /usr/bin보다 앞에 오므로 배포판 기본 clang-cl이 더 낮은 버전이어도
# cargo-xwin은 여기의 LLVM 19 도구를 먼저 사용한다.
mkdir -p /usr/local/bin
for tool in llvm-lib llvm-dlltool llvm-rc clang-cl; do
    if [ -f "/usr/bin/${tool}-${LLVM_VERSION}" ]; then
        ln -sf "/usr/bin/${tool}-${LLVM_VERSION}" "/usr/local/bin/${tool}"
        echo "심볼릭 링크 생성: /usr/local/bin/${tool} → ${tool}-${LLVM_VERSION}"
    else
        echo "경고: /usr/bin/${tool}-${LLVM_VERSION} 을 찾을 수 없습니다"
    fi
done
for tool in clang clang++ lld-link; do
    if [ -f "/usr/bin/${tool}-${LLVM_VERSION}" ]; then
        ln -sf "/usr/bin/${tool}-${LLVM_VERSION}" "/usr/local/bin/${tool}"
        echo "심볼릭 링크 생성: /usr/local/bin/${tool} → ${tool}-${LLVM_VERSION}"
    else
        echo "경고: /usr/bin/${tool}-${LLVM_VERSION} 을 찾을 수 없습니다"
    fi
done

echo ""
echo "설치 완료:"
clang-${LLVM_VERSION} --version | head -1
ld.lld-${LLVM_VERSION} --version | head -1
llvm-lib --version 2>/dev/null | head -1 || echo "llvm-lib: 미설치"
clang-cl --version 2>/dev/null | head -1 || echo "clang-cl: 미설치"
