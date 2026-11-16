#!/usr/bin/env bash
# Builds the Rust core (crates/fob-ffi) for Apple platforms and lays it out
# as the FobCore Swift package expects (docs/apple/plan.md §1):
#
#   apple/core/FobFFI.xcframework                the static library, one slice per platform
#   apple/core/Sources/FobCore/Generated/        the Swift bindings (FobFFI.swift)
#
# Neither is committed: like Android's, the bindings are generated from the
# crates on every build, so they can't drift.
#
#   build-core.sh --sim       the iOS simulator slice (aarch64-apple-ios-sim)
#   build-core.sh --mac       the macOS slice (aarch64-apple-darwin)
#   build-core.sh --release   every slice: iOS device, iOS simulator, macOS
#
# Needs macOS with Xcode, and Rust with the targets the slices name.
# PROFILE=debug builds the slices without --release (quicker for screenshots).
set -euo pipefail

repo="$(cd "$(dirname "$0")/../.." && pwd)"
package="$repo/apple/core"
generated="$package/Sources/FobCore/Generated"
work="$repo/target/apple"

case "${1:-}" in
    --sim) targets=(aarch64-apple-ios-sim) ;;
    --mac) targets=(aarch64-apple-darwin) ;;
    --release) targets=(aarch64-apple-ios aarch64-apple-ios-sim aarch64-apple-darwin) ;;
    *) echo "usage: $0 --sim | --mac | --release" >&2; exit 2 ;;
esac

# The library is built for the apps' minimum versions (apple/project.yml).
export IPHONEOS_DEPLOYMENT_TARGET=26.7
export MACOSX_DEPLOYMENT_TARGET=26.0

profile="${PROFILE:-release}"
cargo_profile=(--release)
if [[ "$profile" == debug ]]; then cargo_profile=(); fi

cd "$repo"
# Cargo doesn't rebuild compiled dependencies when a deployment target
# changes, and the linker then warns that their objects are for a newer OS
# than the app's. So the Apple targets' builds start over when one changes.
stamp="$repo/target/apple-deployment-targets"
want="$IPHONEOS_DEPLOYMENT_TARGET $MACOSX_DEPLOYMENT_TARGET"
if [[ "$(cat "$stamp" 2>/dev/null)" != "$want" ]]; then
    for target in aarch64-apple-ios-sim aarch64-apple-ios aarch64-apple-darwin; do
        rm -rf "$repo/target/$target"
    done
    mkdir -p "$repo/target"
    echo "$want" > "$stamp"
fi

libraries=()
for target in "${targets[@]}"; do
    cargo build --locked "${cargo_profile[@]}" -p fob-ffi --lib --target "$target"
    libraries+=("$repo/target/$target/$profile/libfob_ffi.a")
done

# The bindings come from the first slice's library: UniFFI reads its
# metadata from the library's symbols, which every build of the same source
# shares. Bindgen is its own small crate (apple/core/bindgen), so nothing is
# built for this Mac beyond it.
cargo build --locked --release --manifest-path "$package/bindgen/Cargo.toml" --target-dir "$repo/target/bindgen"
rm -rf "$work" "$generated"
mkdir -p "$work/bindings" "$work/headers" "$generated"
"$repo/target/bindgen/release/uniffi-bindgen" \
    generate --library "${libraries[0]}" --language swift --out-dir "$work/bindings"

# The Swift file into the package; its C header and module map
# (module fob_ffiFFI, which the Swift file imports) into the XCFramework.
cp "$work"/bindings/*.swift "$generated/"
cp "$work"/bindings/*.h "$work/headers/"
cat "$work"/bindings/*.modulemap > "$work/headers/module.modulemap"

args=()
for library in "${libraries[@]}"; do
    args+=(-library "$library" -headers "$work/headers")
done
rm -rf "$package/FobFFI.xcframework"
xcodebuild -create-xcframework "${args[@]}" -output "$package/FobFFI.xcframework"

echo "FobFFI.xcframework (${targets[*]}) and the Swift bindings are in $package"
