# Bonk Arena Smart Contract

## Project Overview

Bonk Arena is a Solana blockchain-based gaming smart contract that implements a complete game leaderboard and reward system. Players can participate in games by paying BONK tokens, compete for rankings, and win rewards.

## Key Features

### 1. Leaderboard System
- Records top 10 players' scores
- Includes player addresses, scores, and custom names
- Automatic sorting and updating

### 2. Game Participation Mechanism
- Entry fee payment using BONK tokens
- Unique game key generation
- 10-minute game time limit
- Score submission and verification

### 3. Reward System
- Dynamic prize pool with configurable ratios (e.g., 70% prize pool, 30% commission)
- Configurable prize distribution for top 3 players (e.g., 50%, 30%, 20%)
- Automated reward distribution

## Technical Architecture

### Smart Contracts
- Main Contract: `bonk_arena` - Handles game logic and leaderboard
- Test Token Contract: `test_token` - For testing purposes

### Account Structure
- Leaderboard Account: Stores game configuration and player rankings
- Game Session Account: Manages individual player sessions
- Token Accounts: Handles BONK token operations

## Build Instructions

### Prerequisites
- Rust 1.70.0 or higher
- Solana CLI tools (latest version)
- Anchor Framework (0.30.1 or higher)
- Node.js and npm/yarn
- Git

### Development Environment Setup

1. Install Rust and Solana