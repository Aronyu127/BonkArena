import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { TestToken } from "../target/types/test_token";
import { PublicKey, Keypair, SystemProgram, SYSVAR_RENT_PUBKEY } from "@solana/web3.js";
import { TOKEN_PROGRAM_ID, createAssociatedTokenAccount, getAssociatedTokenAddress } from "@solana/spl-token";

describe("test_token", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const program = anchor.workspace.TestToken as Program<TestToken>;
  const mintKeypair = Keypair.generate();
  const authority = provider.wallet;
  let tokenAccount: PublicKey;

  it("Initialize", async () => {
    // 创建代币
    await program.methods
      .initialize()
      .accounts({
        mint: mintKeypair.publicKey,
        authority: authority.publicKey,
        payer: authority.publicKey,
        systemProgram: SystemProgram.programId,
        tokenProgram: TOKEN_PROGRAM_ID,
        rent: SYSVAR_RENT_PUBKEY,
      })
      .signers([mintKeypair])
      .rpc();

    // 创建代币账户
    tokenAccount = await getAssociatedTokenAddress(
      mintKeypair.publicKey,
      authority.publicKey
    );

    await createAssociatedTokenAccount(
      provider.connection,
      authority.payer,
      mintKeypair.publicKey,
      authority.publicKey
    );
  });

  it("Mint tokens", async () => {
    // 铸造 1000 个代币（考虑 9 位小数）
    const amount = new anchor.BN(1000 * 1e9);

    await program.methods
      .mintTokens(amount)
      .accounts({
        mint: mintKeypair.publicKey,
        tokenAccount: tokenAccount,
        authority: authority.publicKey,
        tokenProgram: TOKEN_PROGRAM_ID,
      })
      .rpc();
  });
}); 