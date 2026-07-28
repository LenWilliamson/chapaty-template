#!/usr/bin/env bash
# Local image build check. Run before pushing a Dockerfile change:
#   ./bin/build-images.sh [base|agent|tearsheet|all] [--amd64]
#
# The real builds happen in CI, which is also the only thing that pushes. This
# script exists so that a Dockerfile you just edited can be proved to build at
# all, with the layer order and the COPY paths it claims, without waiting for a
# tag and a CI run to find out.
set -euo pipefail

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Every path below is relative to the repository root, and the Docker build
# context is that root. The script moves there first rather than depending on
# where it was called from.
cd "$(git rev-parse --show-toplevel)"

# The default is a local placeholder rather than the real registry. The
# name only affects the tag written on a local image. This script never
# pushes, so a placeholder is enough to check that a Dockerfile builds.
#
# Set the real value to tag images the way CI does. It is the same value as the
# ARTIFACT_REGISTRY repository variable:
#   REGISTRY=europe-west3-docker.pkg.dev/<project>/<repo> ./bin/build-images.sh
REGISTRY="${REGISTRY:-chapaty-local}"

# ==============================================================================
# Arguments
# ==============================================================================

TARGET="all"
PLATFORM_ARGS=()
CROSS_BUILD=0

# Every docker build below expands PLATFORM_ARGS in the ${name[@]+"${name[@]}"}
# form rather than the plain "${name[@]}". The array is empty on a native build,
# and the bash that ships with macOS is old enough to treat an empty array
# expansion as an unset variable, which set -u then turns into a fatal error.
# The plus form expands to nothing when the array is empty and to the quoted
# elements when it is not.

# The flag is accepted in any position, so that both of these read naturally:
#   ./bin/build-images.sh tearsheet --amd64
#   ./bin/build-images.sh --amd64 tearsheet
for arg in "$@"; do
    case "$arg" in
        base|agent|tearsheet|all)
            TARGET="$arg"
            ;;
        --amd64)
            # Without this, the build targets whatever the host is. That is the
            # right default for a correctness check, but it does not produce the
            # artifact that Cloud Run would actually run.
            CROSS_BUILD=1
            PLATFORM_ARGS=(--platform linux/amd64)
            ;;
        -h|--help)
            echo "Usage: ./bin/build-images.sh [base|agent|tearsheet|all] [--amd64]"
            echo ""
            echo "  base       Rust dependency build environment (slow)"
            echo "  agent      per run strategy image, needs the base image locally"
            echo "  tearsheet  Python QuantStats report image"
            echo "  all        all three, in dependency order (default)"
            echo ""
            echo "  --amd64    build for linux/amd64 instead of this machine"
            echo ""
            echo "Environment:"
            echo "  REGISTRY   image name prefix (default: $REGISTRY)"
            exit 0
            ;;
        *)
            echo -e "${RED}[FAIL] Unknown argument: $arg${NC}"
            echo "       Usage: ./bin/build-images.sh [base|agent|tearsheet|all] [--amd64]"
            exit 1
            ;;
    esac
done

# ==============================================================================
# Version
# ==============================================================================

# The version is read once, here, and every tag below is derived from it. Reading
# the file separately will cause inconsistent tags on a Cargo.toml edit part way
# through a run. The result: Two images that are meant to ship together with two
# different versions.
VERSION="$(grep -m1 '^version = ' Cargo.toml | cut -d'"' -f2)"
if [ -z "$VERSION" ]; then
    echo -e "${RED}[FAIL] Could not read the version from Cargo.toml.${NC}"
    exit 1
fi

# A cargo version can carry build metadata after a plus sign, as 1.3.5+2 does,
# and a plus sign is not a legal character in a Docker tag. It becomes a dash,
# which is the same rewrite CI applies, so local and published tags match.
TAG="${VERSION//+/-}"

BASE_IMAGE="${REGISTRY}/btrun-base:${TAG}"
TEARSHEET_IMAGE="${REGISTRY}/btrun-tearsheet:${TAG}"

# This tag is only ever built on a developer machine and is never pushed. It
# exists solely to prove that Dockerfile.agent still builds after an edit.

AGENT_IMAGE="${REGISTRY}/btrun:local"

echo -e "${BLUE}>>> Building chapaty images locally (no push)...${NC}"
echo "    Version:  $VERSION"
echo "    Tag:      $TAG"
echo "    Registry: $REGISTRY"

if [ "$CROSS_BUILD" -eq 1 ]; then
    echo "    Platform: linux/amd64"
    # Worth saying out loud, because the base image compiles every Rust
    # dependency plus four bundled C libraries. Emulated, that is the difference
    # between a coffee and an afternoon.
    echo -e "${YELLOW}[WARN] --amd64 on a non amd64 machine runs under emulation.${NC}"
    echo -e "${YELLOW}       The Rust images will be much slower to build this way.${NC}"
else
    echo "    Platform: native ($(uname -m))"
fi

BUILT=()

# ==============================================================================
# base
# ==============================================================================

if [ "$TARGET" = "base" ] || [ "$TARGET" = "all" ]; then
    echo -e "\n${YELLOW}[base] Building $BASE_IMAGE ...${NC}"
    docker build ${PLATFORM_ARGS[@]+"${PLATFORM_ARGS[@]}"} \
        -f deploy/docker/Dockerfile.base \
        -t "$BASE_IMAGE" \
        .
    echo -e "${GREEN}[OK] base image built.${NC}"
    BUILT+=("$BASE_IMAGE")
fi

# ==============================================================================
# agent
# ==============================================================================

if [ "$TARGET" = "agent" ] || [ "$TARGET" = "all" ]; then
    # The agent build starts FROM the base image and compiles offline, so the
    # base has to exist locally first. Saying so here is friendlier than letting
    # docker fail on a pull from a registry this script never pushes to.
    if ! docker image inspect "$BASE_IMAGE" > /dev/null 2>&1; then
        echo -e "${RED}[FAIL] The agent image needs $BASE_IMAGE locally.${NC}"
        echo "       Please run: ./bin/build-images.sh base"
        exit 1
    fi

    # In production the context is a directory holding one generated strategy
    # file. That shape is reproduced here. We use the template strategy from
    # this repository. It is the same file the base image carries as its
    # placeholder, so this proves the build works without inventing anything.
    AGENT_CONTEXT="$(mktemp -d)"
    # The temporary context is removed however the script leaves this point,
    # including on a failed build.
    trap 'rm -rf "$AGENT_CONTEXT"' EXIT
    cp src/agents/template/agent.rs "$AGENT_CONTEXT/agent.rs"

    echo -e "\n${YELLOW}[agent] Building $AGENT_IMAGE ...${NC}"
    docker build ${PLATFORM_ARGS[@]+"${PLATFORM_ARGS[@]}"} \
        --build-arg "BASE_IMAGE=$BASE_IMAGE" \
        -f deploy/docker/Dockerfile.agent \
        -t "$AGENT_IMAGE" \
        "$AGENT_CONTEXT"
    echo -e "${GREEN}[OK] agent image built.${NC}"
    BUILT+=("$AGENT_IMAGE")
fi

# ==============================================================================
# tearsheet
# ==============================================================================

if [ "$TARGET" = "tearsheet" ] || [ "$TARGET" = "all" ]; then
    # This one takes the repository root as its context, like the base image,
    # because it copies out of visualization/.
    echo -e "\n${YELLOW}[tearsheet] Building $TEARSHEET_IMAGE ...${NC}"
    docker build ${PLATFORM_ARGS[@]+"${PLATFORM_ARGS[@]}"} \
        -f deploy/docker/Dockerfile.tearsheet \
        -t "$TEARSHEET_IMAGE" \
        .
    echo -e "${GREEN}[OK] tearsheet image built.${NC}"
    BUILT+=("$TEARSHEET_IMAGE")
fi

# ==============================================================================
# Done
# ==============================================================================

# The script stops here on purpose. Pushing is CI's job
echo -e "\n${GREEN}>>> Built ${#BUILT[@]} image(s). Nothing was pushed.${NC}"
# Guarded the same way as PLATFORM_ARGS, so that an empty list prints nothing
# rather than tripping set -u on the older bash that macOS ships.
for image in ${BUILT[@]+"${BUILT[@]}"}; do
    echo "    $image"
done
