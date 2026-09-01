use rand::rngs::StdRng;
use rand::Rng;
use rand::SeedableRng;

/// 随机源抽象：生产环境用 OS 随机种子，测试环境注入固定序列实现可复现对局。
pub trait RngSource {
    /// 返回 `[0, bound)` 内的一个整数。
    fn next_below(&mut self, bound: u32) -> u32;
}

/// 基于 `StdRng` 的默认实现。
pub struct StdRngSource(pub StdRng);

impl StdRngSource {
    pub fn from_os_rng() -> Self {
        Self(StdRng::from_entropy())
    }
}

impl RngSource for StdRngSource {
    fn next_below(&mut self, bound: u32) -> u32 {
        self.0.gen_range(0..bound)
    }
}
