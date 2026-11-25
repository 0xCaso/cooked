//! Open at login: the app's own `SMAppService.mainApp`. Outside a bundle (`cargo run`) registering fails and Setup
//! goes on without it; the error comes back as one line.

use cooked_model::setup_model::LoginItem;
use objc2::rc::Retained;
use objc2_service_management::{SMAppService, SMAppServiceStatus};

/// The app's login item. It holds nothing: each call asks `SMAppService` again, so a change in System Settings
/// shows up at the next one.
#[derive(Debug, Clone, Copy, Default)]
pub struct MainAppLogin;

fn main_app() -> Retained<SMAppService> {
    // SAFETY: a class method returning the app's own service object; it registers nothing.
    unsafe { SMAppService::mainAppService() }
}

impl LoginItem for MainAppLogin {
    fn is_enabled(&self) -> bool {
        // SAFETY: reads the service's status; no side effect.
        unsafe { main_app().status() == SMAppServiceStatus::Enabled }
    }

    /// On: register. Off: unregister only when enabled.
    fn set(&mut self, on: bool) -> Result<(), String> {
        let service = main_app();
        // SAFETY: the documented calls on the app's own service; errors come back as `NSError`.
        let done = if on {
            unsafe { service.registerAndReturnError() }
        } else if self.is_enabled() {
            unsafe { service.unregisterAndReturnError() }
        } else {
            Ok(())
        };
        done.map_err(|e| e.localizedDescription().to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Reads the status only: a test binary is no bundle, and nothing here may register a login item.
    #[test]
    fn a_test_binary_is_not_a_login_item() {
        assert!(!MainAppLogin.is_enabled());
    }
}
