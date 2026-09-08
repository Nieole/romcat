//! **任务**：一趟长活的把手——报得出进度、按得下停下、跑在画帧那条线程之外。
//!
//! ## 为什么这一层在核心里
//!
//! 界面只画和转发（ADR-0005）。「一趟活跑到哪儿了」「停下来了没有」「上一趟花了多久」
//! 这几件事全是**领域侧要说清的账**，不是像素——命令行将来也会拿同一份把手把
//! Ctrl-C 接上去。所以把手（[`Handle`]）与**任务台**（[`Board`]）都住在这儿，
//! 界面那一侧只剩两件事：每帧读一次进度画出来，把「停下」那一下转发进来。
//!
//! ## 「能停」比「能取消」严格
//!
//! 取消只要求这趟活别再往下做；**能停**还要求停下的地方是干净的：不留半截状态，
//! 下次接着跑不重做已经完成的部分。这条在这一层落成两件具体的事：
//!
//! - [`Handle::step`] 是**分界处**。长入口在两步之间问一次，被要求停下就当场退出——
//!   于是一步要么整个做完、要么根本没开始，不存在做了一半的步骤。
//! - **停在哪儿是干净的，由那个长入口自己说了算。** 只读的活（**差量预览**）到处都干净：
//!   它一个字节都不写，停下等于什么都没发生。会写东西的活（扫描、同步）各有自己的
//!   续跑依据——扫描的**断点**、同步的**清单**——把手只保证它们在两步之间被叫停。
//!
//! ## 一趟活有四种收场，不是三种
//!
//! [`Ending`] 是**一条轴**：跑完了 / 停了什么都没留下 / **停在半路** / 出错了。
//! 第三档不是多余的——写过东西的活被叫停时照旧要把这一趟收完（扫描的**断点**、
//! 同步的**清单**是下一趟接着跑的依据，抛错会把它们丢掉，ADR-0015），于是它交出来的
//! 产物长得跟「跑完了」那一份一模一样。少了这一档，那一趟只能记成「完成」，
//! 而子库屏同时说「⚠️ 这一趟被你按停了」——同一趟活在两屏上说两套话。
//!
//! 走哪一档由**那个长入口自己说**（[`Handle::halfway`]），不由任务台猜：
//! 只有它知道自己写没写过东西。
//!
//! ## 停下有多快，取决于最长的那一步
//!
//! 把手在两步之间生效，所以「按下停下」到「真的停了」之间最坏是**当前这一步的长度**。
//! 一步内部还想更细的，把 [`Handle::cancel`] 那个信号往下传：扫描、识别、同步走的
//! 就是那条路（它们本来就收 [`CancelToken`]）。
//!
//! ## 任务台一次只跑一趟
//!
//! [`Board`] 是一条队列加一个正在跑的位子，不是线程池。理由是这些活几乎都在读同一份
//! 中立库、写同一块盘，同时跑两趟只会互相抢——排队等着才是对的。

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crate::scan::CancelToken;

/// 被要求停下了。**停在两步之间**，因此这不是失败，是一种干净的收场。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Halted;

impl std::fmt::Display for Halted {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("按停下了：停在两步之间，没留下半截状态。")
    }
}

impl std::error::Error for Halted {}

impl From<Halted> for String {
    fn from(halted: Halted) -> Self {
        halted.to_string()
    }
}

/// 一趟活眼下走到哪儿了。**界面每帧读一次这个快照**，读的时候不挡住干活那条线程。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Progress {
    /// 眼下在做的是哪一步，一句话。还没开始时是空的。
    pub step: String,
    /// 这是第几步（从 1 起）。0 表示还没开始。
    pub at: u32,
    /// 一共几步；说不出来时是 0。
    pub steps: u32,
    /// 这一步里头做完了几件。
    pub done: u64,
    /// 这一步里头一共几件；说不出来时是 0。
    pub total: u64,
}

impl Progress {
    /// 走完了几成，0.0 到 1.0。**说不出来时是 `None`**——那时界面该画一条来回跑的条，
    /// 而不是一条停在 0% 的条。
    #[must_use]
    pub fn fraction(&self) -> Option<f32> {
        if self.steps == 0 || self.at == 0 {
            return None;
        }
        // 已经走完的那几步各算一整份，眼下这一步按它自己的进度折算。
        let inner = if self.total == 0 {
            0.0
        } else {
            (self.done as f64 / self.total as f64).clamp(0.0, 1.0)
        };
        let at = f64::from(self.at.min(self.steps)) - 1.0;
        Some(((at + inner) / f64::from(self.steps)).clamp(0.0, 1.0) as f32)
    }

    /// 排成给人看的一句话，如 `3/10 读清单`。
    #[must_use]
    pub fn render(&self) -> String {
        if self.at == 0 {
            return "还没开始".to_string();
        }
        let mut line = if self.steps == 0 {
            format!("第 {} 步 {}", self.at, self.step)
        } else {
            format!("{}/{} {}", self.at, self.steps, self.step)
        };
        if self.total > 0 {
            line.push_str(&format!("（{}/{}）", self.done, self.total));
        }
        line
    }
}

#[derive(Debug, Default)]
struct Shared {
    cancel: CancelToken,
    progress: Mutex<Progress>,
    /// 报过「**停在半路**」没有，报了的话留下了什么。见 [`Handle::halfway`]。
    left_behind: Mutex<Option<String>>,
}

/// 一趟长活的**把手**：干活那一侧报进度、看有没有被叫停；界面那一侧读进度、按停下。
///
/// 克隆出来的是同一个把手（内部共享），所以交一份给后台线程、留一份在界面上是安全的。
#[derive(Debug, Clone, Default)]
pub struct Handle(Arc<Shared>);

impl Handle {
    /// 开一个没人叫停、还没开始的把手。
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// 接着一个**已经在手的中断信号**开一个把手。
    ///
    /// 命令行是这条路的用处：Ctrl-C 早在开工之前就接在一个 [`CancelToken`] 上了，
    /// 而长入口收的是把手。两者共用同一个信号，于是 Ctrl-C 与界面上那个「停下」
    /// 按下去是同一件事。
    #[must_use]
    pub fn with_cancel(cancel: CancelToken) -> Self {
        Self(Arc::new(Shared {
            cancel,
            progress: Mutex::new(Progress::default()),
            left_behind: Mutex::new(None),
        }))
    }

    /// 声明这一趟一共几步。**开工时说一次**，进度条才画得出「走了几成」。
    pub fn steps(&self, steps: u32) {
        if let Ok(mut progress) = self.0.progress.lock() {
            progress.steps = steps;
        }
    }

    /// 走到下一步：报出这一步叫什么，顺手看一眼有没有被叫停。
    ///
    /// **这是那个分界处**——长入口每两步之间调一次，于是停下来的地方永远在两步之间。
    ///
    /// # Errors
    /// 已经被要求停下时返回 [`Halted`]，这一步一件事都还没做。
    pub fn step(&self, name: &str) -> Result<(), Halted> {
        if self.0.cancel.is_cancelled() {
            return Err(Halted);
        }
        if let Ok(mut progress) = self.0.progress.lock() {
            progress.at += 1;
            progress.step = name.to_string();
            progress.done = 0;
            progress.total = 0;
        }
        Ok(())
    }

    /// 这一步里头走到哪儿了。步数多的那种活（几万个文件）拿它报细粒度进度。
    pub fn tick(&self, done: u64, total: u64) {
        if let Ok(mut progress) = self.0.progress.lock() {
            progress.done = done;
            progress.total = total;
        }
    }

    /// 只看一眼有没有被叫停，不换步。一步内部想停得更快的走这条。
    ///
    /// # Errors
    /// 已经被要求停下时返回 [`Halted`]。
    pub fn check(&self) -> Result<(), Halted> {
        if self.0.cancel.is_cancelled() {
            return Err(Halted);
        }
        Ok(())
    }

    /// 报一句：这一趟**停在半路**了——没走完就收了场，可它交得出产物。
    ///
    /// `left_behind` 说的是**留下了什么**，一句话：走了几步、落了几件、断点写下了。
    /// 任务台拿它把这一趟记成 [`Ending::Halfway`] 而不是 [`Ending::Done`]。
    /// **整句话都由这一层给**——任务台一个字都不替它做主（它也不知道这一趟写没写过东西）。
    ///
    /// **写过东西的长入口没走完就收场时都得说这一句。** 不说的话它交出来的产物会被
    /// 当成「跑完了」那一份，于是任务屏历史写着「完成」，而这一趟其实只走了一半——
    /// 那正是 `sync::execute::run` 与 `scan::scan` 两条路上的账（ADR-0015：它们
    /// 被叫停时不抛错，而是记一笔照常返回，好让**清单**与**断点**留得下来）。
    ///
    /// **报了这一句就得返回 `Ok`。** 任务台只在 `Ok` 那一支问它（[`Board`] 的
    /// `settle`），所以报完再把错抛出去的话，这句话被整条丢掉，历史反过来说
    /// 「停了，什么都没留下、可以当没跑过」——而它其实写过东西，那正是 ADR-0015
    /// 最怕的那件事换了个出口。
    ///
    /// 只读的活不该说这一句：它们停下来什么都没留下，那是 [`Ending::Stopped`]。
    pub fn halfway(&self, left_behind: impl Into<String>) {
        if let Ok(mut slot) = self.0.left_behind.lock() {
            *slot = Some(left_behind.into());
        }
    }

    /// 报过「停在半路」没有，报了的话留下了什么。任务台收场时问的就是它。
    ///
    /// **不对外**：这一格是长入口报给任务台的**侧信道**，外面该看的是任务台交出去的
    /// [`Ending::Halfway`]。
    fn left_behind(&self) -> Option<String> {
        self.0.left_behind.lock().ok().and_then(|slot| slot.clone())
    }

    /// 请求停下。界面上那个「停下」按钮按的就是它。
    pub fn stop(&self) {
        self.0.cancel.cancel();
    }

    /// 有没有人按过停下。
    #[must_use]
    pub fn stopped(&self) -> bool {
        self.0.cancel.is_cancelled()
    }

    /// 底下那个**中断信号**。
    ///
    /// 扫描、识别、同步本来就收 [`CancelToken`]，把它交下去，一步内部也停得动。
    #[must_use]
    pub fn cancel(&self) -> &CancelToken {
        &self.0.cancel
    }

    /// 眼下的进度快照。界面每帧读一次。
    #[must_use]
    pub fn progress(&self) -> Progress {
        self.0
            .progress
            .lock()
            .map(|progress| progress.clone())
            .unwrap_or_default()
    }
}

/// 一趟活是怎么收场的——**一条轴，四档**。
///
/// 轴上分的是**留下了什么**，不是「顺不顺利」：
///
/// | 收场 | 留下了什么 |
/// |---|---|
/// | [`Done`](Self::Done) | 跑完了，产物在这儿 |
/// | [`Stopped`](Self::Stopped) | 停了，什么都没留下——干净，可以当没跑过 |
/// | [`Halfway`](Self::Halfway) | **停在半路**：停了，可交出了产物，且明说它是半截的 |
/// | [`Failed`](Self::Failed) | 出错了：停在哪一步、为什么 |
///
/// ## 第三档不是多余的
///
/// 写过东西的活被叫停时照旧要把这一趟收完——扫描的**断点**、同步的**清单**是下一趟
/// 接着跑的依据，抛错会把它们丢掉（ADR-0015）。少了这一档，那一趟只能折成「跑完了」：
/// 任务屏历史写着「完成」，而子库屏同时写着「⚠️ 这一趟被你按停了」——同一趟活在两屏上
/// 说两套话。哪一趟走这一档由**长入口自己说**（[`Handle::halfway`]），不由任务台猜：
/// 只有它知道自己写没写过东西。
///
/// ## 记进历史的是摘掉产物的那一份
///
/// 产物归**认领它的那一屏**，历史只留账，所以 [`Record`] 上那一格是 `Ending<()>`
/// （[`forget`](Self::forget) 折出来的）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ending<T> {
    /// 跑完了，这是它的产物。
    Done(T),
    /// 停了，**什么都没留下**：停在两步之间，状态是干净的，可以当没跑过。
    Stopped,
    /// **停在半路**：没走完就收了场，可交出了产物——那份产物是真的，认领的人照旧要收。
    Halfway {
        /// 收场之前落下来的那份产物。**丢了下一趟就接不上**（ADR-0015）。
        product: T,
        /// 留下了什么，一句话：走了几步、落了几件、断点写下了。**整句由长入口自己说**
        /// （[`Handle::halfway`]）——只有它知道自己写过什么，也只有它知道为什么收的手。
        left_behind: String,
    },
    /// 出错了：停在哪一步、为什么。**不静默结束**，两样都要说得出来。
    Failed {
        /// 出错时正在做的那一步。说不出来时是空的。
        step: String,
        /// 为什么。给人看的一句话。
        why: String,
    },
}

impl<T> Ending<T> {
    /// 排成给人看的一句话。**四档四句，谁都不许长得跟谁一样。**
    #[must_use]
    pub fn render(&self) -> String {
        match self {
            Self::Done(_) => "完成".to_string(),
            Self::Stopped => "按停了".to_string(),
            // **「为什么收的手」不由这一层写死**：眼下报这一句的两条都是被按停的，
            // 而下一批候选（连着失败太多次主动停了、刮削撞上配额）不是——那半句话
            // 归长入口，这儿只管把它摆进「停在半路」这一档里。
            Self::Halfway { left_behind, .. } => format!("停在半路：{left_behind}"),
            Self::Failed { step, why } if step.is_empty() => format!("失败：{why}"),
            Self::Failed { step, why } => format!("在「{step}」这一步失败：{why}"),
        }
    }

    /// 把产物摘掉，只留下账：[`Record`] 上那一格用的就是它。
    #[must_use]
    pub fn forget(&self) -> Ending<()> {
        match self {
            Self::Done(_) => Ending::Done(()),
            Self::Stopped => Ending::Stopped,
            Self::Halfway { left_behind, .. } => Ending::Halfway {
                product: (),
                left_behind: left_behind.clone(),
            },
            Self::Failed { step, why } => Ending::Failed {
                step: step.clone(),
                why: why.clone(),
            },
        }
    }
}

/// 任务台上的一条历史。
#[derive(Debug, Clone)]
pub struct Record {
    /// 任务号。
    pub id: u64,
    /// 这趟活叫什么。
    pub name: String,
    /// 花了多久。
    pub elapsed: Duration,
    /// 怎么收场的。**产物已经归认领它的那一屏**，这儿只留账。
    pub ending: Ending<()>,
}

/// 正在跑的那一趟：名字、已经跑了多久、走到哪一步了。
#[derive(Debug, Clone)]
pub struct Live {
    /// 任务号。
    pub id: u64,
    /// 这趟活叫什么。
    pub name: String,
    /// 已经跑了多久。
    pub elapsed: Duration,
    /// 走到哪一步了。
    pub progress: Progress,
    /// 有没有已经按过停下（按下去到真的停之间还有一步的距离）。
    pub stopping: bool,
}

/// 跑完了的一趟，连它的产物。
#[derive(Debug)]
pub struct Finished<T> {
    /// 任务号。**排活的人拿它认领自己那一趟。**
    pub id: u64,
    /// 这趟活叫什么。
    pub name: String,
    /// 花了多久。
    pub elapsed: Duration,
    /// 收场：跑完了与**停在半路**都带着产物。
    pub ended: Ending<T>,
}

/// 一趟活本身：收一个把手，交出一份产物或者一句给人看的错话。
type Job<T> = Box<dyn FnOnce(&Handle) -> Result<T, String> + Send>;

/// 一趟排上队、还没开跑的活。
struct Queued<T> {
    id: u64,
    name: String,
    job: Job<T>,
}

/// 正在跑的那一趟。
struct Running<T> {
    id: u64,
    name: String,
    handle: Handle,
    /// 主线程这边什么时候把它排上去的。**只用来画「已用多久」**——
    /// 记进历史的那个数由干活那条线程自己掐表（见 [`Board::start_next`]）。
    started: Instant,
    /// 干完之后带回来的：产物，以及**那条线程自己量的耗时**。
    thread: JoinHandle<(Result<T, String>, Duration)>,
}

/// **任务台**：排队、进度、可停、历史。**它不发起操作，只承接。**
///
/// 一次只跑一趟，其余排队等着（见模块注释）。历史留在内存里——它是「上次大概多久」
/// 这个问题的答案，不是账本，关掉窗口就没了。
pub struct Board<T> {
    queued: VecDeque<Queued<T>>,
    running: Option<Running<T>>,
    /// 已经跑完、还没被认领的那几趟。就地跑的那种活直接落在这儿。
    finished: VecDeque<Finished<T>>,
    history: Vec<Record>,
    next: u64,
}

impl<T> Default for Board<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Board<T> {
    /// 开一张空的任务台。
    #[must_use]
    pub fn new() -> Self {
        Self {
            queued: VecDeque::new(),
            running: None,
            finished: VecDeque::new(),
            history: Vec::new(),
            next: 0,
        }
    }

    fn take_id(&mut self) -> u64 {
        self.next += 1;
        self.next
    }

    /// 有活在跑或者排着吗。
    #[must_use]
    pub fn busy(&self) -> bool {
        self.running.is_some() || !self.queued.is_empty()
    }

    /// 有跑完了**还没被认领**的吗。
    ///
    /// 界面拿它决定「这一帧画完之后还要不要再画一帧」：[`Board::run_here`] 就地跑完的
    /// 那一趟，结果落进这一格的时刻已经在本帧问过任务台之后了。不再画一帧的话，
    /// 那份产物要等到下一次有输入事件才交得出去——按钮一直禁着，看着像卡住了。
    #[must_use]
    pub fn settled(&self) -> bool {
        !self.finished.is_empty()
    }

    /// 正在跑的那一趟。
    #[must_use]
    pub fn running(&self) -> Option<Live> {
        self.running.as_ref().map(|running| Live {
            id: running.id,
            name: running.name.clone(),
            elapsed: running.started.elapsed(),
            progress: running.handle.progress(),
            stopping: running.handle.stopped(),
        })
    }

    /// 排着队还没轮到的那几趟，按先来后到。
    #[must_use]
    pub fn queued(&self) -> Vec<(u64, String)> {
        self.queued
            .iter()
            .map(|job| (job.id, job.name.clone()))
            .collect()
    }

    /// 历史，**最近的在前面**，各自带耗时。
    #[must_use]
    pub fn history(&self) -> &[Record] {
        &self.history
    }

    /// 把历史清了。跑着的那一趟不受影响。
    pub fn clear_history(&mut self) {
        self.history.clear();
    }
}

impl<T: Send + 'static> Board<T> {
    /// 排一趟活上去，**跑在画帧那条线程之外**。返回任务号。
    ///
    /// 台上空着就当场开跑，不然排队等着。活本身收一个 [`Handle`]：报进度走它，
    /// 看有没有被叫停也走它。
    pub fn queue(
        &mut self,
        name: impl Into<String>,
        job: impl FnOnce(&Handle) -> Result<T, String> + Send + 'static,
    ) -> u64 {
        let id = self.take_id();
        self.queued.push_back(Queued {
            id,
            name: name.into(),
            job: Box::new(job),
        });
        self.start_next();
        id
    }

    /// **就地跑一趟**：不开线程，在调用者这条线程上跑完，账照样进任务台。
    ///
    /// 这条路是给**分不出第二份中立库连接**的场合留的——合成数据那份库只活在内存里，
    /// `rusqlite` 的连接不是 `Sync`，后台线程拿不到它。那时窗口确实会僵住，
    /// 但合成数据上这些活都是几毫秒的事，而**真库一律走 [`queue`](Self::queue)**。
    ///
    /// **它不排队**：就地跑就是当场跑完，台上那个位子照旧归后台那趟用。所以别拿它跑
    /// 会与台上那趟抢同一份库的活——它存在的前提正是「这份库小到几毫秒就走完」。
    pub fn run_here(
        &mut self,
        name: impl Into<String>,
        job: impl FnOnce(&Handle) -> Result<T, String>,
    ) -> u64 {
        let id = self.take_id();
        let name = name.into();
        let handle = Handle::new();
        let started = Instant::now();
        let result = job(&handle);
        self.settle(id, name, started.elapsed(), &handle, result);
        id
    }

    /// 让某一趟停下。还排着队没开跑的直接撤掉，跑着的把停下的信号递进去。
    ///
    /// 跑着的那一趟**不会当场消失**：它要走到下一个分界处才停得干净，
    /// 那之后 [`poll`](Self::poll) 才收得到它。
    pub fn stop(&mut self, id: u64) {
        if let Some(running) = &self.running
            && running.id == id
        {
            running.handle.stop();
            return;
        }
        if let Some(at) = self.queued.iter().position(|job| job.id == id) {
            let job = self.queued.remove(at).expect("刚找到的位置");
            // **撤掉的那一趟照样交回给排它的人**：不然排活的那一屏会一直记着
            // 「我还有一趟在排」，那个按钮就再也按不动了。
            self.history.insert(
                0,
                Record {
                    id: job.id,
                    name: job.name.clone(),
                    elapsed: Duration::ZERO,
                    ending: Ending::Stopped,
                },
            );
            self.finished.push_back(Finished {
                id: job.id,
                name: job.name,
                elapsed: Duration::ZERO,
                ended: Ending::Stopped,
            });
        }
    }

    /// 每帧问一次：跑完的那一趟把产物交出来，队里下一趟顶上。
    ///
    /// 返回 `None` 只是说「这一帧没有谁跑完」，不代表台上是空的。
    pub fn poll(&mut self) -> Option<Finished<T>> {
        if let Some(done) = self.finished.pop_front() {
            return Some(done);
        }
        let finished = self
            .running
            .as_ref()
            .is_some_and(|running| running.thread.is_finished());
        if !finished {
            return None;
        }
        let running = self.running.take()?;
        // **耗时取干活那条线程自己量的那个数**，不是主线程发现它结束的那一刻。
        // 后者等于「真实耗时 + 最多一个轮询间隔」——窗口被遮住、一帧都不画的时候，
        // 那个间隔能是好几秒，而历史里那一行会照着虚高的数说「上次花了这么久」。
        let (result, elapsed) = match running.thread.join() {
            Ok(pair) => pair,
            // 线程炸了。**不静默结束**：这也是一种失败，得说出口。
            // 它没能带回自己量的耗时，只好退回主线程这边的表。
            Err(_) => (Err("那条线程炸了。".to_string()), running.started.elapsed()),
        };
        self.settle(running.id, running.name, elapsed, &running.handle, result);
        self.start_next();
        self.finished.pop_front()
    }

    /// 把一趟的结果折成收场、记进历史、放进待认领那一格。
    fn settle(
        &mut self,
        id: u64,
        name: String,
        elapsed: Duration,
        handle: &Handle,
        result: Result<T, String>,
    ) {
        // **被按停的那一趟按「停了」记，不按「失败」记。** 两者在界面上长得一样的话，
        // 用户会以为自己按坏了什么。
        //
        // 判据是**那句话正是 [`Halted`] 交出来的那一句**，不是「按过停下就算停了」：
        // 按下停下之后、走到下一个分界处之前，活本身也可能真的出错（卡拔了、盘满了）。
        // 只看 `handle.stopped()` 的话那条错误文本会被整条丢掉，界面上却说
        // 「一个字节都没动，再排一次就是」——那是骗人。
        //
        // **交出了产物的那一趟还要再分一次**：报过「停在半路」（[`Handle::halfway`]）
        // 的走 [`Ending::Halfway`]，没报过的才是「跑完了」。写过东西的活被叫停时
        // 走的正是前者——它照旧交出产物（清单、断点），可这一趟只走了一半。
        let halted = Halted.to_string();
        let ended = match result {
            Ok(product) => match handle.left_behind() {
                Some(left_behind) => Ending::Halfway {
                    product,
                    left_behind,
                },
                None => Ending::Done(product),
            },
            Err(why) if handle.stopped() && why == halted => Ending::Stopped,
            Err(why) => Ending::Failed {
                step: handle.progress().step,
                why,
            },
        };
        self.history.insert(
            0,
            Record {
                id,
                name: name.clone(),
                elapsed,
                ending: ended.forget(),
            },
        );
        self.finished.push_back(Finished {
            id,
            name,
            elapsed,
            ended,
        });
    }

    fn start_next(&mut self) {
        if self.running.is_some() {
            return;
        }
        let Some(job) = self.queued.pop_front() else {
            return;
        };
        let handle = Handle::new();
        let theirs = handle.clone();
        let run = job.job;
        let thread = std::thread::spawn(move || {
            let at = Instant::now();
            let result = run(&theirs);
            (result, at.elapsed())
        });
        self.running = Some(Running {
            id: job.id,
            name: job.name,
            handle,
            started: Instant::now(),
            thread,
        });
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU32, Ordering};

    use super::*;

    /// 等这张台上那一趟跑完，最多等几秒。
    fn 等到跑完<T: Send + 'static>(board: &mut Board<T>) -> Finished<T> {
        for _ in 0..1_000 {
            if let Some(done) = board.poll() {
                return done;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        panic!("五秒都没跑完");
    }

    #[test]
    fn 被按停却交出了产物的那一趟记成停在半路而不是完成() {
        // 写过东西的活（扫描、同步）被叫停时照旧要把这一趟收完——**断点**与**清单**
        // 是下一趟接着跑的依据，抛错会把它们丢掉（ADR-0015）。少了「停在半路」这一档，
        // 那一趟就只能折成「跑完了」：任务屏历史写着「完成」，而子库屏同时写着
        // 「⚠️ 这一趟被你按停了」——同一趟活在两屏上说两套话。
        let mut board: Board<u32> = Board::new();
        board.queue("同步 · 掌机", |task| {
            task.steps(3);
            task.step("新增 SFC/幻想传说 汉化版.zip")?;
            task.stop();
            task.halfway("按停时落了 1 件，清单记着到这儿为止的样子");
            Ok(7)
        });
        let done = 等到跑完(&mut board);
        let Ending::Halfway {
            product,
            left_behind,
        } = done.ended
        else {
            panic!("该是停在半路");
        };
        // **产物照旧交出来**：那份清单非落库不可，丢了下一趟就接不上。
        assert_eq!(product, 7);
        assert!(left_behind.contains("落了 1 件"), "{left_behind}");

        // 历史那一行如实说，而且**不说「完成」**。
        let 记的 = &board.history()[0].ending;
        assert!(matches!(记的, Ending::Halfway { .. }), "{记的:?}");
        let 画出来的 = 记的.render();
        assert!(画出来的.contains("停在半路"), "{画出来的}");
        assert!(
            画出来的.contains("落了 1 件"),
            "说不出留下了什么：{画出来的}"
        );
        assert_ne!(画出来的, Ending::<()>::Done(()).render());
    }

    #[test]
    fn 四种收场各画各的话() {
        // 一条轴四档，谁都不许长得跟谁一样：两档在界面上撞脸，那句「跑了 X 秒」
        // 就成了骗人的话。
        let 四句 = [
            Ending::Done(()).render(),
            Ending::<()>::Stopped.render(),
            Ending::<()>::Halfway {
                product: (),
                left_behind: "按停时落了 12 件".to_string(),
            }
            .render(),
            Ending::<()>::Failed {
                step: "看一眼目标".to_string(),
                why: "卡不在位".to_string(),
            }
            .render(),
        ];
        let 去重: std::collections::BTreeSet<&String> = 四句.iter().collect();
        assert_eq!(去重.len(), 4, "四种收场里有两种画出来是同一句：{四句:?}");
    }

    #[test]
    fn 按下停下之后才真出错的那一趟按失败记而不是按停了记() {
        // 按下停下到走到下一个分界处之间，活本身也可能真的出错（卡拔了、盘满了）。
        // 只看「按过停下没有」的话，那条错误文本会被整条丢掉，界面上却说
        // 「一个字节都没动，再排一次就是」——那是骗人。
        let mut board: Board<()> = Board::new();
        board.queue("会出错的一趟", |task| {
            task.stop();
            Err("目标看不了：卡拔了".to_string())
        });
        let done = 等到跑完(&mut board);
        let Ending::Failed { why, .. } = &done.ended else {
            panic!("该是失败，实际是 {:?}", done.ended);
        };
        assert_eq!(why, "目标看不了：卡拔了");

        // 真在分界处停下的那一趟照旧按「停了」记。
        board.queue("干净停下的一趟", |task| {
            task.stop();
            task.step("下一步")?;
            Ok(())
        });
        let done = 等到跑完(&mut board);
        assert!(matches!(done.ended, Ending::Stopped), "{:?}", done.ended);
    }

    #[test]
    fn 报得出走到第几步() {
        let handle = Handle::new();
        handle.steps(4);
        handle.step("读选择集").expect("没人叫停");
        assert_eq!(handle.progress().at, 1);
        assert_eq!(handle.progress().step, "读选择集");
        handle.tick(3, 6);
        let progress = handle.progress();
        assert_eq!(progress.render(), "1/4 读选择集（3/6）");
        // 走完半步：四步里的第一步走了一半 = 一成二五。
        let fraction = progress.fraction().expect("说得出几成");
        assert!((fraction - 0.125).abs() < 1e-6, "{fraction}");
    }

    #[test]
    fn 说不出总步数时进度是没有而不是零() {
        // 「一条停在 0% 的进度条」与「说不出来」是两回事：前者看着像卡住了。
        let handle = Handle::new();
        assert!(handle.progress().fraction().is_none());
        handle.step("走着").expect("没人叫停");
        assert!(handle.progress().fraction().is_none(), "总步数还没说呢");
    }

    #[test]
    fn 被叫停时停在两步之间() {
        // 「能停」比「能取消」严格：停下的那一步**一件事都还没做**。
        static 做了几步: AtomicU32 = AtomicU32::new(0);
        做了几步.store(0, Ordering::SeqCst);
        let mut board: Board<u32> = Board::new();
        let id = board.queue("数到十", |task| {
            task.steps(10);
            for at in 1..=10 {
                task.step(&format!("第 {at} 步"))?;
                做了几步.fetch_add(1, Ordering::SeqCst);
                std::thread::sleep(Duration::from_millis(10));
            }
            Ok(做了几步.load(Ordering::SeqCst))
        });
        // **等它真的走出两步再叫停**，不靠「睡三十毫秒它总该走几步了」——
        // 机器一忙那条线程可能一步都还没排上，那样这条测试就会偶发红。
        for _ in 0..1_000 {
            if board.running().is_some_and(|live| live.progress.at >= 2) {
                break;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        board.stop(id);
        let done = 等到跑完(&mut board);
        assert!(matches!(done.ended, Ending::Stopped), "{:?}", done.ended);
        let 走完的 = 做了几步.load(Ordering::SeqCst);
        assert!(走完的 > 0 && 走完的 < 10, "该停在半路上：{走完的}");
        // 历史里它是「按停了」，不是「失败」。
        assert_eq!(board.history()[0].ending, Ending::Stopped);
        assert_eq!(board.history()[0].id, id);
    }

    #[test]
    fn 失败的那一趟说得出哪一步为什么() {
        let mut board: Board<()> = Board::new();
        board.queue("排差量", |task| {
            task.steps(3);
            task.step("读选择集")?;
            task.step("看一眼目标")?;
            Err("卡不在位".to_string())
        });
        let done = 等到跑完(&mut board);
        let Ending::Failed { step, why } = done.ended else {
            panic!("该是失败");
        };
        assert_eq!(step, "看一眼目标");
        assert_eq!(why, "卡不在位");
        assert_eq!(
            board.history()[0].ending.render(),
            "在「看一眼目标」这一步失败：卡不在位",
        );
    }

    #[test]
    fn 一次只跑一趟其余排队等着() {
        let mut board: Board<u32> = Board::new();
        let 头一趟 = board.queue("头一趟", |_| {
            std::thread::sleep(Duration::from_millis(40));
            Ok(1)
        });
        let 第二趟 = board.queue("第二趟", |_| Ok(2));
        assert_eq!(board.running().expect("有在跑的").id, 头一趟);
        assert_eq!(board.queued(), vec![(第二趟, "第二趟".to_string())]);
        assert_eq!(等到跑完(&mut board).id, 头一趟);
        assert_eq!(等到跑完(&mut board).id, 第二趟);
        assert!(!board.busy());
        // 历史最近的在前面，各自带耗时。
        assert_eq!(board.history().len(), 2);
        assert_eq!(board.history()[0].id, 第二趟);
        assert!(board.history()[1].elapsed >= Duration::from_millis(40));
    }

    #[test]
    fn 还没轮到的那趟撤得掉() {
        let mut board: Board<u32> = Board::new();
        board.queue("头一趟", |_| {
            std::thread::sleep(Duration::from_millis(30));
            Ok(1)
        });
        let 第二趟 = board.queue("第二趟", |_| Ok(2));
        board.stop(第二趟);
        assert!(board.queued().is_empty());
        assert_eq!(board.history()[0].ending, Ending::Stopped);
        // **撤掉的那一趟也交回给排它的人**，不然排它的那一屏会一直等着它。
        let 撤掉的 = board.poll().expect("撤掉的那趟该交回来");
        assert_eq!(撤掉的.id, 第二趟);
        assert!(matches!(撤掉的.ended, Ending::Stopped));
        let done = 等到跑完(&mut board);
        assert!(matches!(done.ended, Ending::Done(1)));
        assert!(!board.busy(), "撤掉的那趟不该被顶上来跑");
    }

    #[test]
    fn 就地跑的那趟账照样进任务台() {
        let mut board: Board<u32> = Board::new();
        let id = board.run_here("就地跑", |task| {
            task.steps(1);
            task.step("干活")?;
            Ok(7)
        });
        let done = board.poll().expect("当场就跑完了");
        assert_eq!(done.id, id);
        assert!(matches!(done.ended, Ending::Done(7)));
        assert_eq!(board.history()[0].ending, Ending::Done(()));
    }

    #[test]
    fn 历史里的耗时是那趟活自己量的不是隔多久才问一次() {
        // 主线程发现它结束的那一刻取表的话，记下来的是「真实耗时 + 隔多久才问一次」。
        // 窗口被遮住、一帧都不画的时候，那个间隔能是好几秒——历史里那一行就会照着
        // 虚高的数说「上次花了这么久」，而那正是这一栏唯一回答得了的问题。
        let mut board: Board<u32> = Board::new();
        board.queue("一下就完", |_| Ok(1));
        // 故意隔很久才问一次。
        std::thread::sleep(Duration::from_millis(300));
        let done = board.poll().expect("早跑完了");
        assert!(
            done.elapsed < Duration::from_millis(100),
            "{:?} 里混进了「隔多久才问一次」",
            done.elapsed,
        );
        assert_eq!(board.history()[0].elapsed, done.elapsed);
    }

    #[test]
    fn 清得掉历史() {
        let mut board: Board<u32> = Board::new();
        board.run_here("跑一趟", |_| Ok(1));
        let _ = board.poll();
        assert_eq!(board.history().len(), 1);
        board.clear_history();
        assert!(board.history().is_empty());
    }
}
