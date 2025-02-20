# Contributing Tests to CTF

This guide provides instructions on how to contribute a test to CTF.

## How to write an ABI test

An ABI test is a test that verifies the ABI or runtime behavior of an API in
the SDK. These tests are distributed as Fuchsia packages containing test
components and run entirely on the target device.

### Prerequisites

* Your test must be written in C, C++, or Rust.
* This guide assumes the reader is familiar with Fuchsia [Packages],
[Components] and [Component Manifests].


### Step 1: Create a test directory

The structure of the `//sdk/ctf` directory mirrors the structure of SDK
artifacts. Your test should go in the same directory as the interface under test
is found in an SDK. For example:

| Tests for... | Should go in... |
|--------------|-----------------|
| Host tools   | //sdk/ctf/tests/tools |
| FIDL interfaces | //sdk/ctf/tests/fidl |
| Libraries    | //sdk/ctf/tests/pkg |

See existing tests under `//sdk/ctf/tests` for examples.

### Step 2: Create your test executable

Note: The CTF build templates verify that dependencies are released in an SDK.
If your test needs an exception, [file a bug] in `DeveloperExperience>CTF`. The
allow list can be found [here](/sdk/ctf/build/allowed_ctf_deps.gni).

In your test directory's `BUILD.gn` file, create a test executable using CTF
build templates.

  * {C/C++}

    ```gn
    import("//sdk/ctf/build/ctf.gni")

    ctf_executable("my_test_binary") {
      deps = [ "//zircon/system/ulib/zxtest" ]
      sources = [ "my_test.cc" ]
      testonly = true
    }
    ```

  * {Rust}

  ```gn
  import("//sdk/ctf/build/ctf.gni")

  ctf_rustc_test("my_test_binary") {
    edition = "2021"
    source_root = "src/my_test.rs"
    sources = [ "src/my_test.rs" ]
  }
  ```

### Step 3: Create your test component

Note: This section assumes familiarity with the concept of [Test Components].

```json5
// my_test_component.cml
{
    include: [
        // Select the appropriate test runner shard here:
        // rust, elf, etc.
        "//src/sys/test_runners/rust/default.shard.cml",
    ],
    program: {
        binary: "bin/my_test_binary",
    },
    facets: {
        // mark your test type "cts".
        "fuchsia.test": { type: "cts" },
    },
    ...
}
```

Wrap your executable as a Fuchsia component. CTF provides a special GN template
for creating a component:

```gn
ctf_fuchsia_component("my_test_component") {
  testonly = true
  manifest = "meta/my_test_component.cml",
  deps = [ ":my_test_binary" ]
}
```

### Step 4: Create your test package

CTF also provides a special GN template for creating a test package:

```gn
ctf_fuchsia_test_package("my_test") {
  package_name = "my_test"
  test_components = [ ":my_test_component" ]
}
```

### Step 5: Run the test

These instructions require you to open several terminal tabs.

#### Tab 1: Start the Fuchsia package server

```
fx serve
```

#### Tab 2: Start the Fuchsia emulator

```
ffx emu start --headless
```

* `--headless` disables graphical output.

For more information on configuring the emulator, see [Start the Fuchsia Emulator].

#### Tab 3: Stream the device logs

This step is useful for debugging your test.

```
ffx log
```

#### Tab 4: Run the test

<pre class="prettyprint">
<code class="devsite-terminal">fx set core.x64 --with <var>TARGET_LABEL</var> </code>
<code class="devsite-terminal">fx test <var>TARGET_LABEL</var> # or fx test <var>TEST_NAME</var> </code>
</pre>

* `-v` enables verbose output.

See the section about "Debugging tips" below.

### Step 6. Verify your test passes as part of the CTF release

This step involves building the CTF in the same way that our CI does when it is
released, then running those CTF tests in your local checkout. It is necessary
because upon release, we automatically rewrite each CTF test package's name to
deduplicate it from the same package's name at HEAD (the build does not allow
duplicate names) and we must verify that the test still passes after this
rewrite.

Follow the instructions in Step 4 to start an emulator and a package server,
then launch a new terminal and run the following command:

```
$FUCHSIA_DIR/sdk/ctf/build/scripts/verify_release/verify_release.py
```

This command will build the CTF archive, release it to your
`//prebuilt/cts/test/*` directory, and run the tests contained therein. After
a brief pause, the test results will be printed to the terminal window.

To learn more about that script, or to run the commands manually, please see
[//sdk/ctf/build/scripts/verify_release/README.md](https://fuchsia.googlesource.com/fuchsia/+/refs/heads/main/sdk/ctf/build/scripts/verify_release/README.md).

Some common causes of test failure at this stage include:

* The test launches one of its child components using an absolute component URL.
  * **Explanation**: The URL is no longer correct because the test's package was
  renamed.
  * **Solution**: Use a [relative component URL] instead.
* The test is missing a dependency.
  * **Explanation**: It's possible that the Fuchsia system contained a different
  set of packages when you built and ran your test at HEAD vs as part of the CTF
  and that your test depended on some of those packages.
  * **Solution**: Make sure your test's GN target explicitly lists all of its
  direct dependencies as `deps`.

If you need additional help debugging at this step, please reach out to the CTF
team by filing a bug in the [CTF bug component].

### Step 7. Make the test run on presubmit

This step causes the version of your test from Fuchsia's HEAD commit to run as
part of Fuchsia's presumbit queue.

Add a "tests" group to your BUILD file:

```
group("tests") {
  testonly = true
  deps = [ ":my_test" ]
}
```

Next add this target as a dependency to the closest ancestor `group("tests")`
target. This will ensure that your test is added to the next CTF release.

### Debugging Tips

* If your test hangs, use `ffx component list -v` to inspect its current state.

## How to write a test for experimental FIDL

The CTF supports writing tests for FIDL interfaces that are not yet released in
the SDK. This allows you to write CTF tests for your API while you are still
developing it. Writing tests using the CTF framework while you develop your API
ensures that your tests do not depend on any non-SDK targets.

### Step 1. Ensure your FIDL is marked as experimental
Your FIDL interface should be marked with `sdk_category = "experimental"`. If
your interface is marked as `public` or `partner`, follow the steps above to
write a CTF test.

```
fidl("fuchsia.interface") {
  sdk_category = "experimental"
  name = "fuchsia.interface"
  sources = [ "interface.fidl" ]
}
```

### Step 2. Add your FIDL to the CTF allow list
The CTF verifies that its dependencies are in the SDK. To avoid build time
errors, your FIDL must be added to the CTF [allow list].

```
# //sdk/ctf/build/allowed_ctf_deps.gni
ALLOWED_EXPERIMENTAL_FIDL = [
  ...,
  "//absolute/path/to/experimental:fidl"
]
```

### Step 3. Follow the steps above to write an ABI test
Follow the steps in the [How to write an ABI test](#how_to_write_an_abi_test)
section to write an ABI test for your experimental FIDL.

### Step 4. Mark your test package as experimental
The CTF includes tests based on metadata generated by
`ctf_fuchsia_test_package`. To avoid generating this metadata (and including
your test in the CTF), mark your test package with `uses_experimental_fidl`.

```
ctf_fuchsia_test_package("my_experimental_test") {
  uses_experimental_fidl = true
  test_components = [ ":my_experimental_component" ]
}
```

### Step 5. Release your CTF test
When your FIDL interface is no longer experimental, release your CTF test!
To release your CTF test, follow the steps in the
[How to write an ABI test](#how_to_write_an_abi_test) section and delete the
`uses_experimental_fidl` line from your `ctf_fuchsia_test_package`.

## How to remove an ABI test

Please see the FAQ section about [retiring tests].

## How to disable your test

Please see the FAQ section about [disabling tests].

[Component Manifests]: /docs/concepts/components/v2/component_manifests.md
[Components]: /docs/concepts/components/v2
[Fuchsia language policy]: /docs/contribute/governance/policy/programming_languages.md
[Packages]: /docs/concepts/packages/package.md
[Start the Fuchsia Emulator]: /docs/get-started/set_up_femu.md
[Test Components]: /docs/development/testing/components/test_component.md
[file a bug]: https://bugs.fuchsia.dev/p/fuchsia/issues/list?q=component%3ADeveloperExperience%3ECTS
[relative component URL]: /docs/reference/components/url.md#relative
[CTF bug component]: https://bugs.fuchsia.dev/p/fuchsia/templates/detail?saved=1&template=Fuchsia%20Compatibility%20Test%20Suite%20%28CTS%29&ts=1627669234
[disabling tests]: /docs/development/testing/ctf/faq.md#disable-a-test
[retiring tests]: /docs/development/testing/ctf/faq.md#retire-a-test
[allow list]: /sdk/ctf/build/allowed_ctf_deps.gni
