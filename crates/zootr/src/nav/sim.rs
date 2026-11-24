//! A fake game for the tests: a point that moves as the stick pushes it,
//! walls that stop it, and hooks to make things happen after a push or a
//! press.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

use anyhow::Result;

use super::CLOCKWISE;
use super::drive::{Drive, SPEED, Sense};
use crate::dev::{Dev, State, Switches};
use crate::live::tests::Fake;
use crate::patch;
use crate::play::Input;

/// A push this short or shorter turns Link where he stands.
const TAP: u64 = 100;
/// How far Link moves between looks at the walls, in m.
const STEP: f32 = 0.05;

type Hook = Box<dyn FnMut(&mut World)>;

/// Link and his surroundings.
pub struct World {
    pub at: [f32; 3],
    pub yaw: f32,
    /// The heading the game keeps once it stops updating it: in water.
    pub stale: Option<f32>,
    pub map: String,
    pub event: bool,
    /// Hearts, in quarters.
    pub hearts: i32,
    /// Rectangles `[x0, z0, x1, z1]` that Link can't enter.
    pub walls: Vec<[f32; 4]>,
    /// Each stick push: its direction and milliseconds.
    pub pushes: Vec<(String, u64)>,
    /// Each button pressed.
    pub presses: Vec<String>,
    pub shots: Vec<PathBuf>,
    /// Runs after each stick push has moved Link.
    pub after_push: Option<Hook>,
    /// Runs after each button press.
    pub after_press: Option<Hook>,
    /// Whether ZL has put the camera behind him since the last push.
    camera: bool,
}

impl World {
    /// A warp leaves the camera behind Link, as ZL does.
    pub fn warped(&mut self) {
        self.camera = true;
    }

    fn solid(&self, at: [f32; 3]) -> bool {
        (self.walls.iter())
            .any(|w| (w[0]..=w[2]).contains(&at[0]) && (w[1]..=w[3]).contains(&at[2]))
    }
}

type Shared = Rc<RefCell<World>>;

/// What the harness reads of the game.
pub struct Eyes(pub Shared);

/// What the harness presses on it.
pub struct Hands(pub Shared);

/// Link at `at` facing `yaw`, in a map named `Test`.
pub fn start(at: [f32; 3], yaw: f32) -> (Eyes, Hands) {
    let world = Rc::new(RefCell::new(World {
        at,
        yaw,
        stale: None,
        map: "Test".into(),
        event: false,
        hearts: 12,
        walls: vec![],
        pushes: vec![],
        presses: vec![],
        shots: vec![],
        after_push: None,
        after_press: None,
        camera: false,
    }));
    (Eyes(world.clone()), Hands(world))
}

/// A dev panel on a game that holds only its hook: any other read fails.
pub fn dev() -> Dev<Fake> {
    let main = 0x10_0000;
    let at = patch::OFFSETS.entries.link_update;
    let hook = patch::site(at).unwrap();
    let game = Fake::new(vec![(main + u64::from(at), hook)]);
    Dev::with(game, main + u64::from(patch::OFFSETS.got.gamedata)).unwrap()
}

/// A drive that never waits.
pub fn drive<'a>(eyes: &'a Eyes, hands: &'a mut Hands) -> Drive<'a, Eyes, Hands> {
    let mut drive = Drive::new(eyes, hands);
    drive.settle = Duration::ZERO;
    drive.wait = Duration::ZERO;
    drive
}

impl Sense for Eyes {
    fn link(&self) -> Result<[f32; 3]> {
        Ok(self.0.borrow().at)
    }

    fn yaw(&self) -> Result<f32> {
        let world = self.0.borrow();
        Ok(world.stale.unwrap_or(world.yaw))
    }

    fn state(&self) -> Result<State> {
        let world = self.0.borrow();
        Ok(State {
            map: world.map.clone(),
            child: true,
            loading: false,
            event: world.event,
            switches: Switches::default(),
        })
    }

    fn hearts(&self) -> Result<i32> {
        Ok(self.0.borrow().hearts)
    }
}

/// Run a hook with the world it changes.
fn run(world: &mut World, hook: fn(&mut World) -> &mut Option<Hook>) {
    if let Some(mut f) = hook(world).take() {
        f(world);
        *hook(world) = Some(f);
    }
}

impl Input for Hands {
    fn running(&self) -> bool {
        true
    }

    fn launch(&mut self, _: Option<&Path>) -> Result<()> {
        Ok(())
    }

    fn keys(&self) -> Result<()> {
        Ok(())
    }

    fn hold(&mut self, button: &str, _ms: u64) -> Result<()> {
        let mut world = self.0.borrow_mut();
        world.presses.push(button.into());
        // A held stick runs where the camera looks, as a push does.
        if button
            .split('+')
            .any(|b| ["LUp", "LDown", "LLeft", "LRight"].contains(&b))
        {
            assert!(world.camera, "a held stick without ZL before it");
        }
        world.camera |= button == "ZL";
        run(&mut world, |w| &mut w.after_press);
        Ok(())
    }

    fn stick(&mut self, direction: &str, ms: u64) -> Result<()> {
        let mut world = self.0.borrow_mut();
        assert!(world.camera, "a push without ZL before it");
        world.camera = false;
        world.pushes.push((direction.into(), ms));
        // A button held along with the stick (a dash) moves him no further.
        let direction = direction.split('+').next().unwrap_or(direction);
        let eighths = CLOCKWISE.iter().position(|&d| d == direction).unwrap();
        world.yaw -= eighths as f32 * std::f32::consts::FRAC_PI_4;
        if ms > TAP {
            let want = SPEED * ms as f32 / 1000.0;
            let (sin, cos) = world.yaw.sin_cos();
            let mut walked = 0.0;
            while walked < want {
                let next = [
                    world.at[0] + sin * STEP,
                    world.at[1],
                    world.at[2] + cos * STEP,
                ];
                if world.solid(next) {
                    break;
                }
                world.at = next;
                walked += STEP;
            }
        }
        run(&mut world, |w| &mut w.after_push);
        Ok(())
    }

    fn shot(&mut self, to: &Path) -> Result<()> {
        self.0.borrow_mut().shots.push(to.to_owned());
        Ok(())
    }
}
