# DriveLang

A domain-specific programming language for image processing and media
pipelines, built in Rust.

DriveLang provides a simple, pipeline-based syntax for building image
processing workflows with a fast and predictable execution model.


------------------------------------------------------------------------



# Installation

Download the latest release for your operating system from the Releases
page.

## Windows

1.  Download DriveLang-v1.0.0-Windows.zip.
2.  Extract the archive.
3.  Open a terminal in the extracted folder.
4.  Run:

    drive.exe run your_file.drive

5. Add drive.exe to PATH

## Linux

Extract the archive and run:

    chmod +x drive
    ./drive run your_file.drive

## macOS

Extract the archive and run:

    chmod +x drive
    ./drive run your_file.drive

------------------------------------------------------------------------

# Package Contents
    drive / drive.exe
    GUIDE.md
    examples/
    stdlib/
     LICENSE

-   GUIDE.md contains the complete language documentation.
-   examples/ contains sample DriveLang programs.
> Do not forget to change the Path in the Examples or they will Not Execute.
-   stdlib/ contains the standard library included with DriveLang.

------------------------------------------------------------------------

# Quick Start

- Run the included example:

    `drive examples/hello.drive`

- Or create a new file:
- ```
    import std::core;

    print("Hello, DriveLang!");
    ```

- Save it as hello.drive and run:

    `drive run hello.drive`

---

# Documentation

- See [GUIDE](GUIDE.md) for the complete language reference, syntax, standard
library, and refer examples for better understanding.

---

# License

- This project is licensed under the Apache-2.0 License.
