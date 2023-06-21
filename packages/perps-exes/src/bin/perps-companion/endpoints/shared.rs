// use cosmos::Contract;
//
// struct ContractQuerier(Contract);
//
// impl<T> ContractQuerier {
//     async fn query<T>(&self, msg: T, query_type: QueryType) -> Result<T, Error>
//         where
//             T: serde::de::DeserializeOwned,
//     {
//         let mut attempt = 1;
//         loop {
//             let res = self.0.query(&msg).await.map_err(|source| {
//                 let e = Error::FailedToQueryContract {
//                     msg: msg.clone(),
//                     query_type,
//                 };
//                 log::error!("Attempt #{attempt}: {e}. {source:?}");
//                 e
//             });
//             match res {
//                 Ok(x) => break Ok(x),
//                 Err(e) => {
//                     if attempt >= 5 {
//                         break Err(e);
//                     } else {
//                         attempt += 1;
//                     }
//                 }
//             }
//         }
//     }
// }
