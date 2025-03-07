# Installation

The PostgresML deployment consists of two parts: the Posgres extension and the dashboard app. The extension provides all the machine learning functionality and can be used independently. The dashboard app provides a system overview for easier management and notebooks for writing experiments.

## Extension

The extension can be installed from our Ubuntu `apt` repository or, if you're using a different distribution, from source.

### Dependencies

#### Python 3.7+

PostgresML 2.0 distributed through `apt` requires Python 3.7 or higher. We use Python to provide backwards compatibility with Scikit and other machine learning libraries from that ecosystem.

You will also need to install the following Python packages:

```
sudo pip3 install xgboost lightgbm scikit-learn
```

#### Python 3.6 and below

If your system Python is older, consider installing a newer version from [`ppa:deadsnakes/ppa`](https://launchpad.net/~deadsnakes/+archive/ubuntu/ppa) or Homebrew. If you don't want to or can't have Python 3.7 or higher on your system, refer to **:material-linux: :material-microsoft: From Source (Linux & WSL)** below for building without Python support.


#### PostgreSQL

PostgresML is a Postgres extension and requires PostgreSQL to be installed. We support PostgreSQL 10 through 14. You can use the PostgreSQL version that comes with your system or get it from the [PostgreSQL PPA](https://wiki.postgresql.org/wiki/Apt).

### Install the extension

=== ":material-ubuntu: Ubuntu"

	1. Add our repository into your sources:

		```bash
		echo "deb [trusted=yes] https://apt.postgresml.org $(lsb_release -cs) main" | sudo tee -a /etc/apt/sources.list
		```

	2. Install the extension:

		```
		export POSTGRES_VERSION=14
		sudo apt-get update && sudo apt-get install -y postgresql-pgml-${POSTGRES_VERSION}
		```

	Both ARM and Intel/AMD architectures are supported.


=== ":material-linux: :material-microsoft: From Source (Linux & WSL)"

	These instructions assume a Debian flavor Linux and PostgreSQL 14. Adjust the PostgreSQL
	version accordingly if yours is different. Other flavors of Linux should work, but have not been tested.
	PostgreSQL 10 through 14 are supported.

	1. Install the latest Rust compiler from [rust-lang.org](https://www.rust-lang.org/learn/get-started).

	2. Install a [modern version](https://apt.kitware.com/) of CMake.

	3. Install PostgreSQL development headers and other dependencies:

		```bash
		export POSTGRES_VERSION=14
		sudo apt-get update && \
		sudo apt-get install -y \
			postgresql-server-${POSTGRES_VERSION}-dev \
			libpython3-dev \
			libclang-dev \
			cmake \
			pkg-config \
			libssl-dev \
			clang \
			build-essential \
			libopenblas-dev \
			python3-dev
		```

	4. Clone our git repository:

		```bash
		git clone https://github.com/postgresml/postgresml && \
		cd postgresml && \
		git submodule update --init --recursive && \
		cd pgml-extension
		```
	
	5. Install [`pgx`](https://github.com/tcdi/pgx) and build the extension (this will take a few minutes):

		**With Python support:**

		```bash
		export POSTGRES_VERSION=14
		cargo install cargo-pgx && \
		cargo pgx init --pg${POSTGRES_VERSION} /usr/bin/pg_config && \
		cargo pgx package && \
		cargo pgx install
		```

		**Without Python support:**

		```bash
		export POSTGRES_VERSION=14
		cp docker/Cargo.toml.no-python Cargo.toml && \
		cargo install cargo-pgx && \
		cargo pgx init --pg${POSTGRES_VERSION} /usr/bin/pg_config && \
		cargo pgx package && \
		cargo pgx install
		```

=== ":material-apple: From Source (Mac)"

	_N.B._: Apple M1s have an issue with `openmp` which XGBoost and LightGBM depend on,
	so presently the extension doesn't work on M1s and M2s. We're tracking this [issue](https://github.com/postgresml/postgresml/issues/364).
	
	1. Install the latest Rust compiler from [rust-lang.org](https://www.rust-lang.org/learn/get-started).

	2. Clone our git repository:

		```bash
		git clone https://github.com/postgresml/postgresml && \
		cd postgresml && \
		git submodule update --init --recursive && \
		cd pgml-extension
		```
	3. Install PostgreSQL and other dependencies:

		```bash
		brew install llvm postgresql cmake openssl pkg-config
		```

		Make sure to follow instructions from each package for post-installation steps.
		For example, `openssl` requires some environment variables set in `~/.zsh` for
		the compiler to find the library.

	4. Install [`pgx`](https://github.com/tcdi/pgx) and build the extension (this will take a few minutes):

		```bash
		cargo install cargo-pgx && \
		cargo pgx init --pg14 /usr/bin/pg_config && \
		cargo pgx package && \
		cargo pgx install
		```


### Enable the extension

#### Update postgresql.conf

PostgresML needs to be preloaded at server startup, so you need to add it into `shared_preload_libraries`:

```
shared_preload_libraries = 'pgml,pg_stat_statements'
```

#### Install into database

Now that the extension is installed on your system, add it into the database where you'd like to use it:

=== "SQL"

	Connect to the database and create the extension:

	```
	CREATE EXTENSION pgml;
	```

=== "Output"

	```
	postgres=# CREATE EXTENSION pgml;
	INFO:  Python version: 3.10.4 (main, Jun 29 2022, 12:14:53) [GCC 11.2.0]
	INFO:  Scikit-learn 1.1.1, XGBoost 1.62, LightGBM 3.3.2
	CREATE EXTENSION
	```


That's it, PostgresML is ready. You can validate the installation by running:

=== "SQL"
	```sql
	SELECT pgml.version();
	```

=== "Output"

	```
	postgres=# select pgml.version();
	      version      
	-------------------
	 2.0.0
	(1 row)
	```

## Dashboard

The dashboard is a Django application. Installing it requires no special dependencies or commands:


=== ":material-linux: :material-microsoft: Linux & WSL"

	Install Python if you don't have it already:

	```bash
	apt-get update && \
	apt-get install python3 python3-pip python3-virtualenv
	```

=== ":material-apple: Mac"

	Install Python if you don't have it already:

	```bash
	brew install python3
	```

1. Clone our repository:

	```bash
	git clone https://github.com/postgresml/postgresml && \
	cd postgresml/pgml-dashboard
	```

2. Setup a virtual environment (recommended but not required):

	```bash
	virtualenv venv && \
	source venv/bin/activate
	```

3. Run the dashboard:

	```bash
	python install -r requirements.txt && \
	python manage.py migrate && \
	python manage.py runserver
	```
