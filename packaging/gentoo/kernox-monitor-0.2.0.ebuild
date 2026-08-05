EAPI=8

CRATES=""

DESCRIPTION="Cross-platform terminal system monitor for KernOX"
HOMEPAGE="https://gitlab.com/malorus-kseroks/monitor"
SRC_URI="https://gitlab.com/malorus-kseroks/monitor/-/archive/v${PV}/monitor-v${PV}.tar.gz -> ${P}.tar.gz"
S="${WORKDIR}/monitor-v${PV}"

LICENSE="GPL-3"
SLOT="0"
KEYWORDS="~amd64 ~arm64"

BDEPEND=">=virtual/rust-1.95"

src_compile() {
	cargo build --release --locked || die
}

src_test() {
	cargo test --release --locked || die
}

src_install() {
	dobin target/release/kernox-monitor
	doman packaging/man/kernox-monitor.1
}
