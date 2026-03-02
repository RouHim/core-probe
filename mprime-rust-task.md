Lets build a simple version of https://github.com/sp00n/CoreCycler for linux cli (gui later)
The idea is to start that cli to identify unstable cpu cores using mprime like core cycler does mit prime95
Also journalctl after a boot gives us valueable infos if a cpu core is unstable journalctl -k -b | grep -iE "mce|hardware error|edac"  
the journalctl may need some fine tuning to identify the cores/issues
include data of mprime statically into to release binary
lets not provide as many configuration preferable none options, other like corecycler, lets go with sensitive defaults
our primary goal is to identify unstable cores and report them
we are linux only
in my latests knowlege mprime cli options are pretty limited, we may have to control mprime via its tui menu thus via key input using stdin :(
you are running on a machine which runs pbo on all cpu cores, some are unstable find them!
make sure to create an AGENTS.md file in the repo which ensures all agents following tightly this task description.
If you need sude, reach out to the user, you are not able/allowed to run sudo commands by yourself!
AMD only!!!
64bit CPU only!!!
Also evaluate if there might be a better option for linux to find broken cpu besides mprime.
Make sure to also read: mprime-latest/readme.txt, mprime-latest/stress.txt, mprime-latest/undoc.txt

## References
* https://github.com/sp00n/CoreCycler
* https://www.mersenne.org/
* https://download.mersenne.ca/gimps/v30/30.19/p95v3019b20.linux64.tar.gz
* https://en.wikipedia.org/wiki/Prime95

## Tech Stack

- Programming Language: Rust only
- when needing a http client use ureq

### Technical Requirements

- External crates are allowed, but keep them as low as possible
- Prefer standard Rust libraries and built-in features to minimize external package usage.
- Evaluate trade-offs before adding any third-party crate.
- When using external crates, make sure to use the very latest stable versions.
- All static files needs to be embedded into the binary
- Must compile and run without errors
- Handle user interactions gracefully
- Implement proper error handling and validation
- Use appropriate Rust idioms and patterns
- Logging: prefer `tracing`/`tracing_subscriber` with contextual spans instead of `println!`.
- Error handling: avoid `unwrap`/`expect` in non-test code; surface actionable errors to the UI.
- Structure code into small, focused rust files without using rust modules
- Each file should encapsulate a single responsibility or closely related functionalities.
- Promote reusability and ease of testing by isolating components.
- Follow the SOLID object-oriented design principles to ensure maintainable and extensible code.
- Emphasize single responsibility, open-closed, Liskov substitution, interface segregation, and dependency inversion
  where applicable.
- Use descriptive names and avoid clever tricks or shortcuts that hinder comprehensibility.
- YAGNI - You Aren't Gonna Need It: Avoid adding functionality until it is necessary.
- Don't write unused code for future features.
- Always run code formatters (`cargo fmt`) and linters (`cargo clippy`) when finishing a task.
- Maintain consistent code style across the project to improve readability and reduce friction in reviews.
- Always use RustTLS for any TLS connections, no OpenSSL.

## Testing Practices

### Test-Driven Development (TDD)

- Prefer write tests before writing the functionality.
- Use tests to drive design decisions and ensure robust feature implementation.

### Behavior-Driven Development (BDD)

- Write tests in a BDD style, focusing on the expected behavior and outcomes.
- Structure tests to clearly state scenarios, actions, and expected results to improve communication and documentation.

