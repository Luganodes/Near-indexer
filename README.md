# NEAR Indexer

## Overview

NEAR Indexer is an open-source project designed to index and analyze data from the NEAR blockchain, with a focus on validator performance and delegator interactions. This tool provides valuable insights for validators, delegators, and researchers interested in the NEAR ecosystem.

Below are some use cases for the NEAR Indexer, detailing how various participants in the NEAR ecosystem can utilize its capabilities:

-   Validators could use the indexer to monitor their rewards, track performance, and gain insights into validator reliability.
-   Explorers could utilize the indexer to visualize network transactions, rewards, and epoch history, providing users with detailed insights into network activity.
-   Wallets/Custodians can integrate the indexer to keep users updated on staking rewards, validator performance, and epoch changes, improving portfolio management.

## Features

-   Transaction fetching and processing
-   Epoch data synchronization
-   Delegator data analysis
-   Validator metrics collection
-   Performance data for validators
-   Reward tracking for delegators and validators

## Database Collections

The project utilizes several MongoDB collections to store and analyze data:

### 1. Transactions Collection

| Field             | Type     | Description                                                  |
| ----------------- | -------- | ------------------------------------------------------------ |
| \_id              | ObjectId | Unique identifier for the document                           |
| transaction_hash  | String   | Hash of the transaction                                      |
| amount            | String   | Amount involved in the transaction                           |
| method            | String   | Method called in the transaction (e.g., "deposit_and_stake") |
| action            | String   | Action performed (e.g., "stake", "unstake")                  |
| type\_            | String   | Type of transaction (e.g., "stake", "unstake")               |
| block_height      | Number   | Block height where the transaction was processed             |
| timestamp         | Date     | Timestamp of the transaction                                 |
| delegator_address | String   | Address of the delegator                                     |
| gas_fee           | Number   | Gas fee for the transaction                                  |

### 2. Delegators Collection

| Field                 | Type     | Description                              |
| --------------------- | -------- | ---------------------------------------- |
| \_id                  | ObjectId | Unique identifier for the document       |
| delegator_id          | String   | Unique identifier for the delegator      |
| validator_account_id  | String   | Account ID of the validator              |
| epoch                 | Number   | Epoch number                             |
| start_block_height    | Number   | Start block height of the epoch          |
| end_block_height      | Number   | End block height of the epoch            |
| timestamp             | Number   | Timestamp of the data                    |
| initial_stake         | String   | Initial stake amount                     |
| auto_compounded_stake | String   | Stake amount after auto-compounding      |
| last_update_block     | Number   | Block height of the last update          |
| epoch_id              | String   | Unique identifier for the epoch          |
| total_rewards_earned  | String   | Total rewards earned since initial stake |
| pending_rewards       | String   | Rewards yet to be withdrawn              |
| tokens_withdrawn      | String   | Total tokens withdrawn                   |

### 3. Validator Metrics Collection

| Field              | Type     | Description                               |
| ------------------ | -------- | ----------------------------------------- |
| \_id               | ObjectId | Unique identifier for the document        |
| validatorAccountId | String   | Account ID of the validator               |
| epoch              | Number   | Epoch number                              |
| epochId            | String   | Unique identifier for the epoch           |
| totalStaked        | String   | Total amount staked with the validator    |
| totalDelegators    | Number   | Total number of delegators                |
| timestamp          | Date     | Timestamp of the data                     |
| apy                | Number   | Annual Percentage Yield for the validator |
| rewards            | String   | Total rewards earned by the validator     |
| uptime             | Number   | Uptime percentage of the validator        |

### 4. Epoch Data Collection

| Field              | Type     | Description                        |
| ------------------ | -------- | ---------------------------------- |
| \_id               | ObjectId | Unique identifier for the document |
| epoch              | Number   | Epoch number                       |
| epochId            | String   | Unique identifier for the epoch    |
| validatorAccountId | String   | Account ID of the validator        |
| startBlockHeight   | Number   | Start block height of the epoch    |
| endBlockHeight     | Number   | End block height of the epoch      |
| timestamp          | Date     | Timestamp of the epoch data        |
| delegators         | Object   | Object containing delegator data   |
| transactions       | Array    | Array of transactions in the epoch |

### 5. Epoch Sync Collection

| Field       | Type     | Description                        |
| ----------- | -------- | ---------------------------------- |
| \_id        | ObjectId | Unique identifier for the document |
| start_block | Number   | Start block of the epoch           |
| end_block   | Number   | End block of the epoch             |
| epoch_id    | String   | Unique identifier for the epoch    |
| timestamp   | Date     | Timestamp of the sync              |

### 6. Validator Performance Collection

| Field               | Type     | Description                                            |
| ------------------- | -------- | ------------------------------------------------------ |
| \_id                | ObjectId | Unique identifier for the document                     |
| validatorId         | String   | The account ID of the validator                        |
| blocksProduced      | Number   | Number of blocks produced by the validator             |
| blocksExpected      | Number   | Number of blocks the validator was expected to produce |
| blockProductionRate | String   | Percentage of expected blocks that were produced       |
| chunksProduced      | Number   | Number of chunks produced by the validator             |
| chunksExpected      | Number   | Number of chunks the validator was expected to produce |
| chunkProductionRate | String   | Percentage of expected chunks that were produced       |
| message             | String   | Additional information about the validator's status    |

## Tracked Metrics

### 1. Transactions

We track all transactions related to stake operations:

-   Delegate (Staking)
-   Undelegate (Unstaking)
-   Gas fees across transactions
-   Other relevant transaction metadata

### 2. Delegator Reward Metrics

For each delegator, we track:

-   Total Rewards Earned (Since day of stake)
-   Tokens Withdrawn
-   Stake related transactions
-   Epoch wise rewards

### 3. Validator Reward Metrics

For validators, we track:

-   Validator APY
-   Validator Rewards
-   Validator Uptime

## Setup and Installation

### Prerequisites

-   Git
-   Rust (latest stable version)
-   MongoDB
-   Docker and Docker Compose (for Docker method only)

### Method 1: Using Docker

1. Clone the repository:
    ```
    git clone https://github.com/your-username/near-indexer.git
    cd near-indexer
    ```
2. Create a `.env` file in the project root and add the following variables:

    ```
    MONGO_URI=your_mongodb_connection_string
    DB_NAME=your_database_name
    VALIDATOR_ACCOUNT_ID=your_validator_account_id
    PRIMARY_RPC=primary_near_rpc_endpoint
    SECONDARY_RPC=secondary_near_rpc_endpoint
    PARALLEL_LIMIT=number_of_parallel_tasks (35 epochs at once by default)
    BATCH_SIZE=batch_size_for_processing (10 by default)
    DELEGATOR_BATCH_SIZE=batch_size_for_delegator_processing
    ```

3. Build and run the Docker container:
    ```
    docker-compose up --build
    ```

This will start the NEAR Indexer and connect it to your MongoDB instance.

### Method 2: Without Docker

1. Clone the repository:

    ```
    git clone https://github.com/your-username/near-indexer.git
    cd near-indexer
    ```

2. Install Rust (if not already installed):

    ```
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
    ```

3. Create a `.env` file in the project root and add the following variables (same as in the Docker method).

4. Build the project:

    ```
    cargo build --release
    ```

5. Run the indexer:
    ```
    cargo run --release
    ```

### Configuration

Regardless of the installation method, you need to set the following environment variables in your `.env` file:

-   `MONGO_URI`: Your MongoDB connection string
-   `DB_NAME`: The name of your MongoDB database
-   `VALIDATOR_ACCOUNT_ID`: The account ID of the validator you're indexing
-   `PRIMARY_RPC`: The primary NEAR RPC endpoint
-   `SECONDARY_RPC`: The secondary NEAR RPC endpoint (for fallback)
-   `PARALLEL_LIMIT`: Number of parallel tasks for processing
-   `BATCH_SIZE`: Batch size for processing blocks
-   `DELEGATOR_BATCH_SIZE`: Batch size for processing delegator data

Ensure these variables are properly set before running the indexer.

### Updating

To update the indexer:

1. Pull the latest changes:

    ```
    git pull origin main
    ```

2. If using Docker, rebuild and restart the container:

    ```
    docker-compose up --build
    ```

3. If not using Docker, rebuild and run the updated version:
    ```
    cargo build --release
    cargo run --release
    ```

Remember to check for any changes in the required environment variables or new dependencies that might have been added.

## Usage

Once the Docker container is running, the NEAR Indexer will automatically start processing blocks, transactions, and epoch data based on the configured parameters. It will store the processed data in the specified MongoDB database.

To query the data, you can use MongoDB queries or develop additional tools to analyze the collected information.

## Development

If you want to make changes to the code and test them:

1. Make your changes in the relevant files.
2. Rebuild the Docker image:
    ```
    docker-compose build
    ```
3. Run the updated container:
    ```
    docker-compose up
    ```

## Contributing

Contributions to the NEAR Indexer project are welcome! Please follow these steps to contribute:

1. Fork the repository
2. Create a new branch for your feature or bug fix
3. Make your changes and commit them with clear, descriptive messages
4. Push your changes to your fork
5. Create a pull request to the main repository

Please ensure that your code follows the existing style and includes appropriate tests and documentation.

## License

This project is licensed under the MIT License. See the [LICENSE](LICENSE) file for details.

## Contact

For questions, suggestions, or support, please open an issue in the GitHub repository or contact the project maintainers.

---
# near-indexer
