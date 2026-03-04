use std::time::Duration;
use tokio::io::{AsyncSeekExt, AsyncWriteExt as _, ErrorKind, SeekFrom};

const PWM_EXPORT_FILE: &str = "/sys/class/pwm/pwmchip0/export";
const PWM_DUTY_CYCLE: &str = "/sys/class/pwm/pwmchip0/pwm0/duty_cycle";
const PWM_PERIOD: &str = "/sys/class/pwm/pwmchip0/pwm0/period";
const PWM_ENABLE: &str = "/sys/class/pwm/pwmchip0/pwm0/enable";

#[derive(Debug)]
pub struct Tape {
    pwm: Pwm,
}

const PERIOD: Duration = Duration::from_millis(20);

impl Tape {
    pub async fn new() -> tokio::io::Result<Self> {
        let mut pwm = Pwm::new().await?;

        pwm.set_state(ActivationState::Disabled).await?;
        pwm.set_period(PERIOD).await?;

        Ok(Self { pwm })
    }

    pub async fn extend(&mut self) -> tokio::io::Result<()> {
        let extension_duty_cycle = PERIOD.mul_f32(0.05);
        let extension_time = Duration::from_secs(4);

        self.pwm.set_state(ActivationState::Disabled).await?;
        self.pwm.set_duty_cycle(extension_duty_cycle).await?;
        self.pwm.set_state(ActivationState::Enabled).await?;

        tokio::time::sleep(extension_time).await;

        self.pwm.set_state(ActivationState::Disabled).await
    }

    pub async fn retract(&mut self) -> tokio::io::Result<()> {
        let retraction_duty_cycle = PERIOD.mul_f32(0.11);
        let retraction_time = Duration::from_secs(6);

        self.pwm.set_state(ActivationState::Disabled).await?;
        self.pwm.set_duty_cycle(retraction_duty_cycle).await?;
        self.pwm.set_state(ActivationState::Enabled).await?;

        tokio::time::sleep(retraction_time).await;

        self.pwm.set_state(ActivationState::Disabled).await
    }
}

#[derive(Debug, Copy, Clone)]
enum ActivationState {
    Disabled = 0,
    Enabled = 1,
}

#[derive(Debug)]
struct Pwm {
    duty_cycle_file: tokio::fs::File,
    period_file: tokio::fs::File,
    enable_file: tokio::fs::File,
}

impl Pwm {
    async fn new() -> tokio::io::Result<Self> {
        let mut opts = tokio::fs::OpenOptions::new();

        opts.write(true).read(false);

        let duty_cycle_file = match opts.open(PWM_DUTY_CYCLE).await {
            Ok(file) => file,
            Err(e) if e.kind() == ErrorKind::NotFound => {
                opts.open(PWM_EXPORT_FILE).await?.write_all(b"0").await?;

                opts.open(PWM_DUTY_CYCLE).await?
            }
            Err(e) => return Err(e),
        };

        let period_file = opts.open(PWM_PERIOD).await?;
        let enable_file = opts.open(PWM_ENABLE).await?;

        Ok(Pwm {
            duty_cycle_file,
            period_file,
            enable_file,
        })
    }

    async fn set_duty_cycle(&mut self, duration: Duration) -> tokio::io::Result<()> {
        let nanos = duration.as_nanos();

        self.duty_cycle_file.seek(SeekFrom::Start(0)).await?;

        self.duty_cycle_file
            .write_all(format!("{nanos}").as_bytes())
            .await
    }

    async fn set_period(&mut self, duration: Duration) -> tokio::io::Result<()> {
        let nanos = duration.as_nanos();

        self.period_file.seek(SeekFrom::Start(0)).await?;

        self.period_file
            .write_all(format!("{nanos}").as_bytes())
            .await
    }

    async fn set_state(&mut self, state: ActivationState) -> tokio::io::Result<()> {
        let raw_state = state as u64;
        self.enable_file.seek(SeekFrom::Start(0)).await?;

        self.enable_file
            .write_all(format!("{raw_state}").as_bytes())
            .await
    }
}
