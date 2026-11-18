//! Flow tests: the app's main paths run end to end against a fake Wine, inside a throwaway HOME.
//!
//! Each flow runs in a child copy of the test binary whose HOME is a fresh folder under the
//! temp dir (see `isolated`), so every path the app derives from HOME lands there and nothing
//! of the person's (bottles, engines, settings, Steam) is ever read or written. The fake Wine
//! (`fake_wine`) answers the commands the app really sends and runs no Windows code. Nothing
//! reaches the network, Finder, Terminal, native dialogs or Mac launchers under `cfg(test)`.

#[macro_use]
mod isolated;
mod driver;
mod fake_wine;
mod fixtures;

#[cfg(test)]
mod apple_flows;
#[cfg(test)]
mod botella_flows;
#[cfg(test)]
mod botellas_heredadas_flows;
#[cfg(test)]
mod capas_flows;
#[cfg(test)]
mod finder_flows;
#[cfg(test)]
mod iconos_flows;
#[cfg(test)]
mod inicio_flows;
#[cfg(test)]
mod instalador_flows;
#[cfg(test)]
mod keyboard_flows;
#[cfg(test)]
mod main_flows;
#[cfg(test)]
mod marca_flows;
#[cfg(test)]
mod palabras_flows;
#[cfg(test)]
mod programas_flows;
#[cfg(test)]
mod soak;
#[cfg(test)]
mod tema_flows;
